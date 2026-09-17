use std::time::Duration;

use anyhow::Result;
use chrono::NaiveDate;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};

use crate::config;
use crate::fetch;
use crate::model::Story;

struct EditionTab {
    name: String,
    stories: Vec<Story>,
    state: ListState,
}

struct App {
    tabs: Vec<EditionTab>,
    active: usize,
    status: String,
    message: Option<String>,

    client: reqwest::blocking::Client,
    start_date: NaiveDate,
    max_back: i64,

    settings_open: bool,
    settings_cursor: usize,
    settings_selected: Vec<bool>,
}

impl App {
    fn new(
        client: reqwest::blocking::Client,
        start_date: NaiveDate,
        max_back: i64,
        groups: Vec<(String, Vec<Story>)>,
        status: String,
    ) -> Self {
        let tabs = groups
            .into_iter()
            .map(|(name, stories)| {
                let mut state = ListState::default();
                if !stories.is_empty() {
                    state.select(Some(0));
                }
                EditionTab { name, stories, state }
            })
            .collect();
        App {
            tabs,
            active: 0,
            status,
            message: None,
            client,
            start_date,
            max_back,
            settings_open: false,
            settings_cursor: 0,
            settings_selected: Vec::new(),
        }
    }

    fn current(&self) -> &EditionTab {
        &self.tabs[self.active]
    }

    fn current_mut(&mut self) -> &mut EditionTab {
        &mut self.tabs[self.active]
    }

    fn selected_story(&self) -> Option<&Story> {
        let tab = self.current();
        tab.state.selected().and_then(|i| tab.stories.get(i))
    }

    fn next(&mut self) {
        let tab = self.current_mut();
        let len = tab.stories.len();
        if len == 0 {
            return;
        }
        let i = match tab.state.selected() {
            Some(i) if i + 1 < len => i + 1,
            Some(_) => 0,
            None => 0,
        };
        tab.state.select(Some(i));
    }

    fn prev(&mut self) {
        let tab = self.current_mut();
        let len = tab.stories.len();
        if len == 0 {
            return;
        }
        let i = match tab.state.selected() {
            Some(0) | None => len - 1,
            Some(i) => i - 1,
        };
        tab.state.select(Some(i));
    }

    fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
            self.message = None;
        }
    }

    fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
            self.message = None;
        }
    }

    fn goto_tab(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active = idx;
            self.message = None;
        }
    }

    fn open_selected(&mut self) {
        if let Some(story) = self.selected_story() {
            let url = story.url.clone();
            let result = std::process::Command::new("open").arg(&url).spawn();
            self.message = Some(match result {
                Ok(_) => format!("Im Browser geöffnet: {url}"),
                Err(e) => format!("Konnte Browser nicht öffnen ({e}): {url}"),
            });
        }
    }

    fn open_settings(&mut self) {
        let current_names: Vec<&str> = self.tabs.iter().map(|t| t.name.as_str()).collect();
        self.settings_selected = config::ALL_EDITIONS
            .iter()
            .map(|(slug, _)| current_names.contains(slug))
            .collect();
        self.settings_cursor = 0;
        self.settings_open = true;
        self.message = None;
    }

    fn settings_cancel(&mut self) {
        self.settings_open = false;
        self.message = None;
    }

    fn settings_next(&mut self) {
        if !config::ALL_EDITIONS.is_empty() {
            self.settings_cursor = (self.settings_cursor + 1) % config::ALL_EDITIONS.len();
        }
    }

    fn settings_prev(&mut self) {
        if !config::ALL_EDITIONS.is_empty() {
            self.settings_cursor = (self.settings_cursor + config::ALL_EDITIONS.len() - 1) % config::ALL_EDITIONS.len();
        }
    }

    fn settings_toggle(&mut self) {
        if let Some(v) = self.settings_selected.get_mut(self.settings_cursor) {
            *v = !*v;
        }
    }

    fn settings_chosen(&self) -> Vec<String> {
        config::ALL_EDITIONS
            .iter()
            .zip(&self.settings_selected)
            .filter(|(_, selected)| **selected)
            .map(|((slug, _), _)| slug.to_string())
            .collect()
    }

    /// Re-fetches the given editions, replaces the current tabs with them,
    /// persists the selection to disk, and closes the settings dialog.
    fn apply_editions(&mut self, editions: Vec<String>) {
        let (groups, status) = fetch::fetch_all(&self.client, &editions, self.start_date, self.max_back);
        self.tabs = groups
            .into_iter()
            .map(|(name, stories)| {
                let mut state = ListState::default();
                if !stories.is_empty() {
                    state.select(Some(0));
                }
                EditionTab { name, stories, state }
            })
            .collect();
        self.active = 0;
        self.status = status;
        self.message = None;
        self.settings_open = false;

        if let Err(e) = config::save(&config::Config { editions }) {
            self.message = Some(format!("Konnte Einstellungen nicht speichern: {e}"));
        }
    }
}

pub fn run(
    client: reqwest::blocking::Client,
    start_date: NaiveDate,
    max_back: i64,
    groups: Vec<(String, Vec<Story>)>,
    status: String,
) -> Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new(client, start_date, max_back, groups, status);
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if app.settings_open {
                    match key.code {
                        KeyCode::Esc => app.settings_cancel(),
                        KeyCode::Char('j') | KeyCode::Down => app.settings_next(),
                        KeyCode::Char('k') | KeyCode::Up => app.settings_prev(),
                        KeyCode::Char(' ') => app.settings_toggle(),
                        KeyCode::Enter => {
                            let chosen = app.settings_chosen();
                            if chosen.is_empty() {
                                app.message = Some("Mindestens eine Edition auswählen.".to_string());
                            } else {
                                app.message = Some("Lade neue Editionen…".to_string());
                                terminal.draw(|f| draw(f, app))?;
                                app.apply_editions(chosen);
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('s') => app.open_settings(),
                    KeyCode::Char('j') | KeyCode::Down => {
                        app.next();
                        app.message = None;
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        app.prev();
                        app.message = None;
                    }
                    KeyCode::Char('g') | KeyCode::Home => {
                        let tab = app.current_mut();
                        if !tab.stories.is_empty() {
                            tab.state.select(Some(0));
                        }
                        app.message = None;
                    }
                    KeyCode::Char('G') | KeyCode::End => {
                        let tab = app.current_mut();
                        let len = tab.stories.len();
                        if len > 0 {
                            tab.state.select(Some(len - 1));
                        }
                        app.message = None;
                    }
                    KeyCode::Enter | KeyCode::Char('o') => app.open_selected(),
                    KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => app.next_tab(),
                    KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => app.prev_tab(),
                    KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                        let idx = c.to_digit(10).unwrap() as usize - 1;
                        app.goto_tab(idx);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn draw(f: &mut Frame, app: &mut App) {
    if app.settings_open {
        draw_settings(f, f.area(), app);
        return;
    }

    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    draw_tabs(f, chunks[0], app);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(chunks[1]);

    draw_list(f, body[0], app);
    draw_detail(f, body[1], app);
    draw_status(f, chunks[2], app);
}

fn category_color(category: &str) -> Color {
    match category {
        "launch" => Color::Green,
        "practical" => Color::Cyan,
        "event" => Color::Yellow,
        "opinion" => Color::Magenta,
        _ => Color::Gray,
    }
}

fn draw_tabs(f: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<Line> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| Line::from(format!(" {} {} ({}) ", i + 1, t.name.to_uppercase(), t.stories.len())))
        .collect();

    let tabs = Tabs::new(titles)
        .select(app.active)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" TLDR — 1-9 / Tab / ←→ wechseln, j/k bewegen, Enter/o öffnen, s Einstellungen, q beenden "),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Black).bg(Color::Cyan))
        .divider("│");

    f.render_widget(tabs, area);
}

fn draw_list(f: &mut Frame, area: Rect, app: &mut App) {
    let active = app.active;
    let tab = &mut app.tabs[active];

    let items: Vec<ListItem> = tab
        .stories
        .iter()
        .map(|s| {
            let line = Line::from(vec![
                Span::styled(format!("{:>3}min ", s.reading_minutes), Style::default().fg(category_color(&s.category))),
                Span::raw(s.title.clone()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = if tab.stories.is_empty() {
        format!(" {} — keine Artikel ", tab.name.to_uppercase())
    } else {
        format!(" {} ", tab.name.to_uppercase())
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray))
        .highlight_symbol("➤ ");

    f.render_stateful_widget(list, area, &mut tab.state);
}

fn draw_detail(f: &mut Frame, area: Rect, app: &App) {
    let text = if let Some(s) = app.selected_story() {
        vec![
            Line::from(Span::styled(s.title.clone(), Style::default().add_modifier(Modifier::BOLD))),
            Line::from(Span::styled(
                format!("{} · {} min · {}", s.domain, s.reading_minutes, s.category),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(s.summary.clone()),
            Line::from(""),
            Line::from(Span::styled(s.url.clone(), Style::default().fg(Color::Blue))),
        ]
    } else {
        vec![Line::from("Keine Artikel in dieser Edition.")]
    };

    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Details "))
        .wrap(Wrap { trim: true });

    f.render_widget(p, area);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    let text = app.message.clone().unwrap_or_else(|| app.status.clone());
    let p = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}

fn draw_settings(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = config::ALL_EDITIONS
        .iter()
        .enumerate()
        .map(|(i, (slug, desc))| {
            let checked = app.settings_selected.get(i).copied().unwrap_or(false);
            let marker = if checked { "[x] " } else { "[ ] " };
            let marker_style = if checked {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let line = Line::from(vec![
                Span::styled(marker, marker_style),
                Span::styled(format!("{slug:<10} "), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(*desc, Style::default().fg(Color::Gray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.settings_cursor));

    let footer_text = app
        .message
        .clone()
        .unwrap_or_else(|| "j/k bewegen, Space togglen, Enter übernehmen & speichern, Esc abbrechen".to_string());

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Editionen auswählen "))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray))
        .highlight_symbol("➤ ");

    f.render_stateful_widget(list, outer[0], &mut list_state);

    let footer = Paragraph::new(footer_text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, outer[1]);
}
