use reqwest::Client;
use scraper::{Html, Selector};

use crate::Error;

pub async fn send_query(client: &Client, query: &str) -> Result<String, Error> {
    let url = format!("https://www.statmuse.com/nba/ask/{query}");

    let html = client.get(&url).send().await?.text().await?;

    log::debug!("html obtained from url: {url}");

    let answer = Html::parse_document(&html)
        .select(
            &Selector::parse(r"body > div.main-layout.mb-5.bg-team-primary.text-team-secondary > div > div.flex-1.flex.flex-col.justify-between.text-center.md\:text-left > h1 > p")
            .expect("selector should be valid")
        )
        .next()
        .ok_or(Error::Other("no answer found in html"))?
        .text()
        .collect::<String>();

    log::debug!("answer obtained: {answer}");

    Ok(answer)
}
