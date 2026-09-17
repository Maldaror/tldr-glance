mod fetch;
mod model;
mod ui;

use anyhow::Result;
use chrono::Local;
use clap::Parser;
use model::Story;

/// Schnelles Terminal-Scannen der täglichen TLDR-Newsletter.
#[derive(Parser, Debug)]
#[command(name = "tldr-glance")]
struct Args {
    /// Kommagetrennte Liste von TLDR-Editionen (siehe tldr.tech/newsletters)
    #[arg(short, long, default_value = "tech,ai,dev")]
    editions: String,

    /// Datum der Ausgabe (YYYY-MM-DD), Standard: heute
    #[arg(short, long)]
    date: Option<String>,

    /// Wie viele Tage rückwärts versucht wird, falls für das Datum keine Ausgabe existiert
    #[arg(long, default_value_t = 5)]
    max_back: i64,

    /// Nur die geparsten Artikel auf stdout ausgeben, keine TUI starten
    #[arg(long)]
    dump: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let start_date = match &args.date {
        Some(d) => chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")?,
        None => Local::now().date_naive(),
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    // One group per edition, kept separate (not merged) so the TUI can show
    // them as distinct tabs instead of one mixed list.
    let mut groups: Vec<(String, Vec<Story>)> = Vec::new();
    let mut status_parts = Vec::new();

    for edition in args.editions.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        match fetch::fetch_edition_latest(&client, edition, start_date, args.max_back) {
            Ok((used_date, stories)) => {
                let note = if used_date == start_date {
                    format!("{edition}: {used_date} ({})", stories.len())
                } else {
                    format!("{edition}: {used_date} [neueste verfügbare] ({})", stories.len())
                };
                status_parts.push(note);
                groups.push((edition.to_string(), stories));
            }
            Err(e) => {
                status_parts.push(format!("{edition}: FEHLER ({e})"));
                groups.push((edition.to_string(), Vec::new()));
            }
        }
    }

    let status = status_parts.join("  |  ");

    if args.dump {
        for (edition, stories) in &groups {
            println!("=== {} ({}) ===", edition.to_uppercase(), stories.len());
            for s in stories {
                println!(
                    "[{}] {}min · {}\n  {}\n  {}\n",
                    s.category, s.reading_minutes, s.title, s.domain, s.url
                );
            }
        }
        println!("--- {status} ---");
        return Ok(());
    }

    if groups.iter().all(|(_, stories)| stories.is_empty()) {
        eprintln!("Keine Artikel gefunden. Status: {status}");
        std::process::exit(1);
    }

    ui::run(groups, status)?;
    Ok(())
}
