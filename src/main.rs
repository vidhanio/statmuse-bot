#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]

mod oauth2;
mod statmuse;

use std::{
    env,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{Extension, Router};
use futures::prelude::*;
use reqwest::Client;
use thiserror::Error;
use tokio::time;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;
use twitter_v2::{
    authorization::{BearerToken, Oauth2Client},
    ApiResponse, Authorization, Tweet, TwitterApi,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("twitter error")]
    Twitter(#[from] twitter_v2::Error),
    #[error("reqwest error")]
    Reqwest(#[from] reqwest::Error),
    #[error("sled error")]
    Sled(#[from] sled::Error),
    #[error("bincode error")]
    Bincode(#[from] bincode::Error),
    #[error("other error")]
    Other(&'static str),
}

async fn reply<A: Authorization + Send + Sync>(
    http_client: &Client,
    twitter_api: &TwitterApi<A>,
    tweet: Tweet,
) -> Result<Option<Tweet>, Error> {
    let text = tweet.text.replace("@statmuse_bot", "").trim().to_string();

    let reply = statmuse::send_query(http_client, &text).await?;

    twitter_api
        .post_tweet()
        .in_reply_to_tweet_id(tweet.id)
        .text(reply)
        .send()
        .await
        .map(ApiResponse::into_data)
        .map_err(Into::into)
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    dotenv::dotenv()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(
            env::var("RUST_LOG").unwrap_or_else(|_| "statmuse_bot=debug,tower_http=debug".into()),
        ))
        .init();

    let http_client = Client::new();

    let stream_api = TwitterApi::new(BearerToken::new(env::var("TWITTER_BEARER_TOKEN")?));

    stream_api
        .post_tweets_search_stream_rule()
        .add_tagged(
            "@statmuse_bot -is:retweet -from:statmuse_bot",
            "mentions statmuse_bot",
        )
        .send()
        .await?;

    let address = SocketAddr::new([127, 0, 0, 1].into(), 3000);

    let ctx = Arc::new(Mutex::new(oauth2::Context {
        client: Oauth2Client::new(
            env::var("TWITTER_CLIENT_ID")?,
            env::var("TWITTER_CLIENT_SECRET")?,
            format!("http://{address}/callback").parse()?,
        ),
        db: sled::open("db")?,
    }));

    let app = Router::new()
        .route("/callback", axum::routing::get(oauth2::callback))
        .route("/login", axum::routing::get(oauth2::login))
        .layer(TraceLayer::new_for_http())
        .layer(Extension(Arc::clone(&ctx)));

    tokio::spawn(async move {
        axum::Server::bind(&address)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
    log::debug!("serving app at: {address}");
    println!("go to http://{address}/login");

    time::sleep(time::Duration::from_secs(10)).await;

    let stream = stream_api.get_tweets_search_stream().stream().await?;

    stream
        .map_ok(|payload| payload.data)
        .for_each(|tweet| {
            let ctx = Arc::clone(&ctx);
            let http_client = http_client.clone();

            async move {
                let ctx = ctx.lock().unwrap();

                match tweet {
                    Ok(tweet) => {
                        if let Some(tweet) = tweet {
                            let token = ctx.token();
                            match token {
                                Some(mut token) => {
                                    let refresh_result =
                                        ctx.client.refresh_token_if_expired(&mut token).await;
                                    match refresh_result {
                                        Ok(true) => ctx.set_token(&token),
                                        Ok(false) => {}
                                        Err(e) => log::error!("error refreshing token: {e}"),
                                    }
                                    let twitter_api = TwitterApi::new(token);

                                    match reply(&http_client, &twitter_api, tweet).await {
                                        Ok(Some(tweet)) => {
                                            log::debug!("replied to tweet with id: {}", tweet.id);
                                        }
                                        Ok(None) => {
                                            log::warn!("no reply");
                                        }
                                        Err(e) => {
                                            log::error!("failed to reply to tweet: {e:?}");
                                        }
                                    }
                                }
                                None => {
                                    log::error!("no token");
                                }
                            }
                        } else {
                            log::error!("no tweet obtained");
                        }
                    }
                    Err(e) => {
                        log::error!("error while getting tweet: {e:?}");
                    }
                }
            }
        })
        .await;

    Ok(())
}
