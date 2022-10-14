use reqwest::Client;
use twitter_v2::{ApiResponse, Authorization, Tweet, TwitterApi};

use crate::{statmuse, Error};

macro_rules! regex {
    ($($re:expr),+ $(,)?) => {($({
         static RE: ::once_cell::sync::OnceCell<::regex::Regex> = ::once_cell::sync::OnceCell::new();
         RE.get_or_init(|| ::regex::Regex::new($re).unwrap())
     }),+)};
}

pub async fn reply<A: Authorization + Send + Sync>(
    http_client: &Client,
    twitter_api: &TwitterApi<A>,
    tweet: &Tweet,
) -> Result<Option<Tweet>, Error> {
    let re = regex!(r"\.?@(\w){1,15}");
    let text = re.replace_all(&tweet.text, "").trim().to_owned();

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
