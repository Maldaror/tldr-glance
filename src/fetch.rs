use anyhow::{bail, Result};
use chrono::NaiveDate;
use scraper::{Html, Selector};
use serde_json::Value;

use crate::model::{RawStory, Story};

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";

/// Fetches every edition in `editions`, keeping them as separate groups
/// (one per tab in the UI) along with a human-readable status line.
pub fn fetch_all(
    client: &reqwest::blocking::Client,
    editions: &[String],
    start_date: NaiveDate,
    max_back: i64,
) -> (Vec<(String, Vec<Story>)>, String) {
    // Editions are independent HTTP round-trips, so fetch them concurrently.
    // `thread::scope` lets the threads borrow `client`/`edition` directly
    // (reqwest's blocking client is Send + Sync) without cloning anything
    // or requiring 'static data.
    let results: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = editions
            .iter()
            .map(|edition| scope.spawn(move || fetch_edition_latest(client, edition, start_date, max_back)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut groups = Vec::new();
    let mut status_parts = Vec::new();

    for (edition, result) in editions.iter().zip(results) {
        match result {
            Ok((used_date, stories)) => {
                let note = if used_date == start_date {
                    format!("{edition}: {used_date} ({})", stories.len())
                } else {
                    format!("{edition}: {used_date} [neueste verfügbare] ({})", stories.len())
                };
                status_parts.push(note);
                groups.push((edition.clone(), stories));
            }
            Err(e) => {
                status_parts.push(format!("{edition}: FEHLER ({e})"));
                groups.push((edition.clone(), Vec::new()));
            }
        }
    }

    (groups, status_parts.join("  |  "))
}

/// Fetches the newest available edition for `edition` starting at `start_date`,
/// walking backwards up to `max_back` days if an edition is missing or empty.
pub fn fetch_edition_latest(
    client: &reqwest::blocking::Client,
    edition: &str,
    start_date: NaiveDate,
    max_back: i64,
) -> Result<(NaiveDate, Vec<Story>)> {
    let mut date = start_date;
    let mut last_err = String::from("unbekannter Fehler");

    for _ in 0..=max_back {
        let url = format!("https://tldr.tech/{edition}/{}", date.format("%Y-%m-%d"));

        // tldr.tech's edge occasionally 404s a valid, just-published URL when
        // hit concurrently with the other editions' requests (same URL
        // succeeds moments later) — retry a couple of times on the same date
        // before concluding the edition genuinely doesn't exist there.
        let mut found = None;
        for attempt in 0..3 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
            match client.get(&url).header(reqwest::header::USER_AGENT, USER_AGENT).send() {
                Ok(resp) if resp.status().is_success() => match resp.text() {
                    Ok(html) => match extract_stories(&html) {
                        Ok(raw_stories) if !raw_stories.is_empty() => {
                            found = Some(raw_stories);
                            break;
                        }
                        Ok(_) => last_err = format!("{url}: keine Artikel im Payload"),
                        Err(e) => last_err = format!("{url}: {e}"),
                    },
                    Err(e) => last_err = format!("{url}: Antwort nicht lesbar ({e})"),
                },
                Ok(resp) => last_err = format!("{url}: HTTP {}", resp.status()),
                Err(e) => last_err = format!("{url}: {e}"),
            }
        }
        if let Some(raw_stories) = found {
            let stories = raw_stories.into_iter().map(Story::from_raw).collect();
            return Ok((date, stories));
        }
        date -= chrono::Duration::days(1);
    }
    bail!("keine Ausgabe für '{edition}' in den letzten {max_back} Tagen gefunden — zuletzt: {last_err}")
}

/// TLDR currently serves two different page templates depending on the
/// edition/date: a newer "keyboard feed" layout (seen on `tech`) that hides
/// story data in embedded JSON, and an older server-rendered newsletter
/// layout (seen on `ai`/`dev`) with the story data directly in the DOM. We
/// try the JSON strategy first and fall back to DOM scraping.
fn extract_stories(html: &str) -> Result<Vec<RawStory>> {
    if let Ok(stories) = extract_stories_new_format(html) {
        if !stories.is_empty() {
            return Ok(stories);
        }
    }
    extract_stories_old_format(html)
}

/// TLDR embeds the full story list as JSON inside Next.js RSC-hydration
/// script chunks (`self.__next_f.push([1,"..."])`). We scan those chunks for
/// the one carrying a `"stories":[...]` payload and parse it directly,
/// instead of scraping the (JS-hydrated, mostly empty at fetch-time) DOM.
///
/// Depending on the page variant, that payload can sit at the top level of
/// the chunk's JSON value, or nested several levels deep inside a
/// serialized React element tree — so once a candidate chunk is found we
/// parse it fully and search the resulting tree for a `stories` key,
/// instead of assuming a fixed shape.
fn extract_stories_new_format(html: &str) -> Result<Vec<RawStory>> {
    let marker = "self.__next_f.push([1,\"";
    let mut search_from = 0usize;

    while let Some(rel_start) = html[search_from..].find(marker) {
        let content_start = search_from + rel_start + marker.len();
        let Some(content_end) = find_unescaped_quote(html, content_start) else {
            break;
        };
        search_from = content_end;

        let escaped_chunk = &html[content_start..content_end];
        let Ok(unescaped) = serde_json::from_str::<String>(&format!("\"{escaped_chunk}\"")) else {
            continue;
        };
        if !unescaped.contains("\"stories\":[") {
            continue;
        }
        let Some(value_start) = unescaped.find(['{', '[']) else {
            continue;
        };
        let mut stream = serde_json::Deserializer::from_str(&unescaped[value_start..]).into_iter::<Value>();
        let Some(Ok(value)) = stream.next() else {
            continue;
        };
        if let Some(stories) = find_stories_array(&value) {
            let raw: Vec<RawStory> = stories
                .iter()
                .filter_map(|v| serde_json::from_value::<RawStory>(v.clone()).ok())
                .collect();
            if !raw.is_empty() {
                return Ok(raw);
            }
        }
    }
    bail!("keine Story-Daten im HTML gefunden (Seitenstruktur evtl. geändert)")
}

/// Depth-first search for the first `"stories": [...]` array anywhere in a
/// parsed JSON value tree.
fn find_stories_array(value: &Value) -> Option<&Vec<Value>> {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(arr)) = map.get("stories") {
                return Some(arr);
            }
            map.values().find_map(find_stories_array)
        }
        Value::Array(arr) => arr.iter().find_map(find_stories_array),
        _ => None,
    }
}

/// Finds the byte index of the next unescaped `"` starting at `from`.
fn find_unescaped_quote(s: &str, from: usize) -> Option<usize> {
    let mut escape = false;
    for (i, c) in s[from..].char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match c {
            '\\' => escape = true,
            '"' => return Some(from + i),
            _ => {}
        }
    }
    None
}

/// Parses the older server-rendered TLDR template: `<section>` blocks with a
/// `<header><h3>Section Name</h3></header>` followed by `<article>` entries,
/// each an `<a><h3>Title (N minute read)</h3></a>` plus a sibling
/// `<div class="newsletter-html">` holding the description as real HTML
/// (real links, real text — no hidden JSON involved here at all).
fn extract_stories_old_format(html: &str) -> Result<Vec<RawStory>> {
    let document = Html::parse_document(html);
    let section_sel = Selector::parse("section").unwrap();
    let header_h3_sel = Selector::parse("header h3").unwrap();
    let article_sel = Selector::parse("article").unwrap();
    let link_sel = Selector::parse("a[href]").unwrap();
    let title_sel = Selector::parse("h3").unwrap();
    let desc_sel = Selector::parse(".newsletter-html").unwrap();

    let mut stories = Vec::new();

    for section in document.select(&section_sel) {
        let section_name = section
            .select(&header_h3_sel)
            .next()
            .map(|h| clean_text(&h.text().collect::<String>()))
            .unwrap_or_else(|| "misc".to_string());

        for article in section.select(&article_sel) {
            let Some(link) = article.select(&link_sel).next() else {
                continue;
            };
            let Some(href) = link.value().attr("href") else {
                continue;
            };
            let Some(title_el) = article.select(&title_sel).next() else {
                continue;
            };
            let raw_title = clean_text(&title_el.text().collect::<String>());
            let (title, reading_minutes) = split_reading_time(&raw_title);

            let summary = article
                .select(&desc_sel)
                .next()
                .map(|d| clean_text(&d.text().collect::<String>()));

            let domain = url::Url::parse(href)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string()));

            stories.push(RawStory {
                url: href.to_string(),
                title,
                summary,
                topic: None,
                category: Some(section_name.clone()),
                canonical_domain: domain,
                estimated_reading_minutes: reading_minutes,
            });
        }
    }

    if stories.is_empty() {
        bail!("altes Template erkannt, aber keine Artikel gefunden (Markup evtl. geändert)");
    }
    Ok(stories)
}

fn clean_text(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Splits a title like `"Some headline (17 minute read)"` into
/// `("Some headline", Some(17))`.
fn split_reading_time(title: &str) -> (String, Option<u32>) {
    if let Some(open) = title.rfind('(') {
        if title.ends_with(')') {
            let inner = &title[open + 1..title.len() - 1];
            if inner.ends_with("read") {
                let digits: String = inner.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(minutes) = digits.parse::<u32>() {
                    return (title[..open].trim_end().to_string(), Some(minutes));
                }
            }
        }
    }
    (title.trim().to_string(), None)
}
