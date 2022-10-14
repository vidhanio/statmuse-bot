use reqwest::Client;
use scraper::{Html, Selector};

use crate::Error;

pub async fn send_query(client: &Client, query: &str) -> Result<(String, String), Error> {
    let url = format!("https://www.statmuse.com/nba/ask/{query}");

    let html = client.get(&url).send().await?.text().await?;

    log::debug!("html obtained from url: {url}");

    // <meta name="description" content="ANSWER_HERE">
    let answer = Html::parse_document(&html)
        .select(&Selector::parse(r"meta[name=description]").expect("should parse selector"))
        .next()
        .expect("should have a meta description tag")
        .value()
        .attr("content")
        .expect("should have content")
        .to_string();

    // <meta property="og:image" content="IMAGE_HERE">
    let image = Html::parse_document(&html)
        .select(&Selector::parse(r"meta[property=og\:image]").expect("should parse selector"))
        .next()
        .expect("should have a meta og:image tag")
        .value()
        .attr("content")
        .expect("should have content")
        .to_string();

    log::debug!("answer and image obtained: {answer}");

    Ok((answer, image))
}
