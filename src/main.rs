mod config;
mod fetch;
mod model;
mod ui;

use anyhow::Result;
use chrono::Local;
use clap::Parser;

/// Schnelles Terminal-Scannen der täglichen TLDR-Newsletter.
#[derive(Parser, Debug)]
#[command(name = "tldr-glance")]
struct Args {
    /// Kommagetrennte Liste von TLDR-Editionen; überschreibt für diesen Lauf
    /// die gespeicherte Auswahl (siehe Einstellungen, Taste 's', oder
    /// tldr.tech/newsletters für alle Slugs)
    #[arg(short, long)]
    editions: Option<String>,

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

    let editions: Vec<String> = match &args.editions {
        Some(s) => s.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        None => config::load().unwrap_or_default().editions,
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let (groups, status) = fetch::fetch_all(&client, &editions, start_date, args.max_back);

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

    ui::run(client, start_date, args.max_back, groups, status)?;
    Ok(())
}
