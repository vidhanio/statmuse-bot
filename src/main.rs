#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]

mod oauth2;
mod statmuse;

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{Extension, Router};
use color_eyre::eyre;
use futures::prelude::*;
use reqwest::Client;
use tokio::time;
use tracing_subscriber::EnvFilter;
use twitter_v2::{authorization::Oauth2Client, ApiResponse, Authorization, Tweet, TwitterApi};

async fn reply<A: Authorization + Send + Sync>(
    http_client: &Client,
    twitter_api: &TwitterApi<A>,
    tweet: Tweet,
) -> eyre::Result<Option<Tweet>> {
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
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let http_client = Client::new();

    let address = SocketAddr::new([127, 0, 0, 1].into(), 8080);

    let ctx = Arc::new(Mutex::new(oauth2::Context {
        client: Oauth2Client::new(
            std::env::var("TWITTER_CLIENT_ID")?,
            std::env::var("TWITTER_CLIENT_SECRET")?,
            format!("http://{address}/callback").parse()?,
        ),
        verifier: None,
        state: None,
        token: None,
    }));

    let app = Router::new()
        .route("/callback", axum::routing::get(oauth2::callback))
        .route("/login", axum::routing::get(oauth2::login))
        .layer(Extension(Arc::clone(&ctx)));

    log::debug!(address = address.to_string(); "serving app");
    axum::Server::bind(&address)
        .serve(app.into_make_service())
        .await?;

    time::sleep(time::Duration::from_secs(10)).await;

    let twitter_api = TwitterApi::new(
        ctx.lock()
            .map_err(|_| eyre::eyre!("failed to lock ctx"))?
            .token
            .as_ref()
            .ok_or_else(|| eyre::eyre!("no token found"))?
            .clone(),
    );

    let stream = twitter_api.get_tweets_search_stream().stream().await?;

    stream
        .map_ok(|payload| payload.data)
        .for_each(|tweet| {
            let twitter_api = twitter_api.clone();
            let http_client = http_client.clone();

            async move {
                match tweet {
                    Ok(tweet) => {
                        if let Some(tweet) = tweet {
                            match reply(&http_client, &twitter_api, tweet).await {
                                Ok(Some(tweet)) => {
                                    log::debug!(tweet = tweet.id.as_u64(); "replied to tweet");
                                }
                                Ok(None) => {
                                    log::debug!("no reply");
                                }
                                Err(err) => {
                                    log::error!(err = err.to_string(); "failed to reply to tweet");
                                }
                            }
                        } else {
                            log::error!("no tweet obtained");
                        }
                    }
                    Err(e) => {
                        log::error!(error = log::as_error!(e); "error while getting tweet");
                    }
                }
            }
        })
        .await;

    Ok(())
}
