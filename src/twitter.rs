use reqwest::Client;
use twitter_v2::{ApiResponse, Authorization, Tweet, TwitterApi};

use crate::{statmuse, Error};

pub async fn reply<A: Authorization + Send + Sync>(
    http_client: &Client,
    twitter_api: &TwitterApi<A>,
    tweet: &Tweet,
) -> Result<Option<Tweet>, Error> {
    let text = tweet.text.replace("@statmuse_bot", "").trim().to_string();

    let (reply, _) = statmuse::send_query(http_client, &text).await?;

    twitter_api
        .post_tweet()
        .in_reply_to_tweet_id(tweet.id)
        .text(reply)
        .send()
        .await
        .map(ApiResponse::into_data)
        .map_err(Into::into)
}
