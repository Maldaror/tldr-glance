# tldr-glance

Ein schnelles Terminal-UI zum täglichen Scannen der [TLDR](https://tldr.tech)-Newsletter.

## Warum

tldr.tech hat vor kurzem ein neues, minimalistisches Design bekommen: Die Artikel-Zusammenfassungen
werden dort nicht mehr direkt angezeigt, sondern erst nach Klick auf eine Zeile nachgeladen. Das
kostet beim täglichen Durchscannen mehrerer Ausgaben spürbar Zeit. `tldr-glance` holt die Ausgaben
stattdessen direkt vom Terminal aus, zeigt alle Artikel samt Zusammenfassung sofort an und lässt sich
komplett über die Tastatur bedienen.

Das Clippen interessanter Artikel bleibt bewusst manuell über den gewohnten Browser-Web-Clipper —
`tldr-glance` öffnet auf Wunsch nur den Artikel im Standardbrowser, extrahiert aber selbst keine
Artikel-Inhalte. Ein Versuch, das programmatisch nachzubauen, hat gezeigt, dass automatisierte
Extraktion (verlinkte Quellen, eingebettete Bilder, exakte Zitate) merklich an Qualität einbüßt
gegenüber einem echten Web-Clipper.

## Installation

Setzt eine lokale Rust-Toolchain voraus ([rustup.rs](https://rustup.rs)).

```sh
cargo build --release
./target/release/tldr-glance
```

Oder direkt über Cargo starten:

```sh
cargo run -- [optionen]
```

## Bedienung

| Taste | Aktion |
|---|---|
| `1`–`9` | Direkt zu Edition (Tab) Nr. N springen |
| `Tab` / `→` / `l` | Nächste Edition |
| `Shift+Tab` / `←` / `h` | Vorherige Edition |
| `j` / `↓` | Nächster Artikel |
| `k` / `↑` | Vorheriger Artikel |
| `g` / `Pos1` | Zum ersten Artikel springen |
| `G` / `Ende` | Zum letzten Artikel springen |
| `Enter` / `o` | Markierten Artikel im Standardbrowser öffnen |
| `s` | Einstellungen (Editionen auswählen) öffnen |
| `q` / `Esc` | Beenden (bzw. Einstellungen ohne Speichern verlassen) |

### Einstellungen

Mit `s` öffnet sich eine Liste aller verfügbaren TLDR-Editionen (`tech`, `ai`, `dev`, `infosec`,
`devops`, `data`, …). `j`/`k` bewegt den Cursor, `Leertaste` togglet eine Edition an/aus, `Enter`
übernimmt die Auswahl, lädt sie sofort neu und speichert sie dauerhaft; `Esc` verwirft die Änderung.

## Konfiguration

Die zuletzt in den Einstellungen gewählten Editionen werden gespeichert unter:

```
~/Library/Application Support/tldr-glance/config.toml
```

```toml
editions = ["tech", "ai", "dev"]
```

Die Datei kann auch von Hand editiert werden. Alle gültigen Slugs stehen unter
[tldr.tech/newsletters](https://tldr.tech/newsletters).

## CLI-Optionen

```
tldr-glance [OPTIONEN]

-e, --editions <LISTE>   Kommagetrennte Editionen für diesen Lauf, überschreibt die
                          gespeicherte Auswahl (z.B. "tech,ai,devops")
-d, --date <YYYY-MM-DD>   Datum der Ausgabe, Standard: heute
    --max-back <N>        Wie viele Tage rückwärts versucht wird, falls für das Datum
                          keine Ausgabe existiert (Standard: 5)
    --dump                Nur die geparsten Artikel auf stdout ausgeben, keine TUI starten
```

`--dump` ist nützlich zum Debuggen oder für ein schnelles `grep` ohne TUI:

```sh
cargo run -- --editions tech,ai --dump | grep -i azure
```

## Funktionsweise

tldr.tech ist aktuell mitten in einer Design-Migration und liefert je nach Edition/Datum eines von
zwei unterschiedlichen Templates aus:

- **Neues "Keyboard Feed"-Design** (aktuell z. B. bei `tech`): Die eigentlichen Artikeldaten
  (Titel, Zusammenfassung, Kategorie, Lesezeit) stecken nicht sichtbar im HTML, sondern als JSON in
  einem eingebetteten Next.js-Hydration-Script (`self.__next_f.push(...)`). `tldr-glance` sucht in
  diesen Chunks nach dem `"stories"`-Array und parst es direkt.
- **Altes, serverseitig gerendertes Template** (aktuell z. B. bei `ai`, `dev`): Artikel stehen als
  ganz normales HTML im DOM (`<section><header><h3>Sektion</h3></header><article>...</article>`),
  inklusive echter Links und Beschreibungstexte. Hier wird ganz klassisch das DOM geparst.

`tldr-glance` probiert beide Strategien automatisch und nutzt, was für die jeweilige Seite passt.

Da tldr.tech für ein noch nicht veröffentlichtes Datum trotzdem `HTTP 200` mit einer
Client-seitigen "Nicht gefunden"-Seite liefert (kein echtes 404), läuft der Fetcher bei Bedarf
automatisch Tag für Tag rückwärts, bis eine Ausgabe mit tatsächlichen Artikeln gefunden wird.

## Bekannte Einschränkungen

- Kategorie-Färbung in der Artikelliste kennt nur die Tags des neuen Templates
  (`launch`/`practical`/`event`/`opinion`); Sektionsnamen aus dem alten Template werden grau
  dargestellt.
- Keine Volltextsuche/Filterung innerhalb einer Edition (bisher nicht gebraucht bei der
  überschaubaren Artikelzahl pro Tag).
