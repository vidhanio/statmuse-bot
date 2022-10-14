#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]

mod oauth2;
mod statmuse;
mod twitter;

use std::{
    env,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{Extension, Router};
use futures::prelude::*;
use reqwest::Client;
use thiserror::Error;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;
use twitter_v2::{
    authorization::{BearerToken, Oauth2Client},
    TwitterApi,
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

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    drop(dotenv::dotenv());
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

    let address = SocketAddr::new([0, 0, 0, 0, 0, 0, 0, 0].into(), env::var("PORT")?.parse()?);

    let url = env::var("URL")?;
    let ctx = Arc::new(Mutex::new(oauth2::Context {
        client: Oauth2Client::new(
            env::var("TWITTER_CLIENT_ID")?,
            env::var("TWITTER_CLIENT_SECRET")?,
            format!("{url}/callback").parse()?,
        ),
        db: sled::open("/data")?,
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
    log::debug!("serving app at: {url}");
    println!("go to {url}/login");

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
                            let token = ctx.refresh_token().await;
                            match token {
                                Ok(Some(token)) => {
                                    let twitter_api = TwitterApi::new(token);

                                    match twitter::reply(&http_client, &twitter_api, &tweet).await {
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
                                Ok(None) => {
                                    log::error!("no token");
                                }
                                Err(e) => {
                                    log::error!("failed to refresh token: {e:?}");
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
