use reqwest::Client;
use scraper::{Html, Selector};

use crate::Error;

pub async fn send_query(client: &Client, query: &str) -> Result<(String, String), Error> {
    let url = format!("https://www.statmuse.com/nba/ask/{query}");

    let html = client.get(&url).send().await?.text().await?;

    log::debug!("html obtained from url: {url}");

    let answer = Html::parse_document(&html)
        .select(&Selector::parse(r"h1").expect("should parse selector"))
        .next()
        .expect("should have h1")
        .text()
        .collect::<String>();

    // <meta property="og:image" content="IMAGE_HERE">
    let image = Html::parse_document(&html)
        .select(&Selector::parse(r"meta[property=og\:image]").expect("should parse selector"))
        .next()
        .expect("should have a meta og:image tag")
        .value()
        .attr("content")
        .expect("should have content")
        .to_string();

    log::debug!("answer obtained: {answer}");

    Ok((answer, image))
}
