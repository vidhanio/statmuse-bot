use color_eyre::eyre;
use reqwest::Client;
use scraper::{Html, Selector};

pub async fn send_query(client: &Client, query: &str) -> color_eyre::Result<String> {
    let url = format!("https://www.statmuse.com/nba/ask/{query}");

    let html = client.get(&url).send().await?.text().await?;

    log::debug!(url = url.as_str(); "html obtained from url");

    let answer = Html::parse_document(&html)
        .select(
            &Selector::parse(r"body > div.main-layout.mb-5.bg-team-primary.text-team-secondary > div > div.flex-1.flex.flex-col.justify-between.text-center.md\:text-left > h1 > p")
            .expect("selector should be valid")
        )
        .next()
        .ok_or_else(|| eyre::eyre!("no answer found in html"))?
        .text()
        .collect::<String>();

    log::debug!(url = url, answer = answer; "answer obtained");

    Ok(answer)
}
