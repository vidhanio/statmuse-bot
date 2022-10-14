use reqwest::Client;
use scraper::{Html, Selector};

use crate::Error;

pub async fn send_query(client: &Client, query: &str) -> Result<String, Error> {
    let url = format!("https://www.statmuse.com/nba/ask?q={query}");

    let html = client.get(&url).send().await?.text().await?;

    log::debug!("html obtained from url: {url}");

    let answer = Html::parse_document(&html)
        .select(&Selector::parse(r"h1").expect("should parse selector"))
        .next()
        .ok_or(Error::Other("no h1 element found"))?
        .text()
        .collect::<String>();

    Ok(answer)
}
