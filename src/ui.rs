use std::io::stdout;
use std::time::Duration;

use anyhow::Result;
use chrono::NaiveDate;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEventKind,
};
use crossterm::execute;
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

    browser: Option<String>,

    settings_open: bool,
    settings_tab: usize,
    settings_cursor: usize,
    settings_selected: Vec<bool>,
    settings_browsers: Vec<String>,
    browser_cursor: usize,
}

const SETTINGS_TABS: [&str; 2] = ["Editionen", "Browser"];

impl App {
    fn new(
        client: reqwest::blocking::Client,
        start_date: NaiveDate,
        max_back: i64,
        groups: Vec<(String, Vec<Story>)>,
        status: String,
        browser: Option<String>,
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
            browser,
            settings_open: false,
            settings_tab: 0,
            settings_cursor: 0,
            settings_selected: Vec::new(),
            settings_browsers: Vec::new(),
            browser_cursor: 0,
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
            let result = config::open_url(&url, self.browser.as_deref());
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
        self.settings_tab = 0;
        self.settings_browsers = config::available_browsers(self.browser.as_deref());
        self.browser_cursor = self
            .browser
            .as_deref()
            .and_then(|name| self.settings_browsers.iter().position(|b| b == name))
            .unwrap_or(0);
        self.settings_open = true;
        self.message = None;
    }

    fn browser_next(&mut self) {
        self.browser_cursor = (self.browser_cursor + 1) % self.settings_browsers.len();
    }

    fn browser_prev(&mut self) {
        self.browser_cursor = (self.browser_cursor + self.settings_browsers.len() - 1) % self.settings_browsers.len();
    }

    fn settings_tab_next(&mut self) {
        self.settings_tab = (self.settings_tab + 1) % SETTINGS_TABS.len();
    }

    fn settings_tab_prev(&mut self) {
        self.settings_tab = (self.settings_tab + SETTINGS_TABS.len() - 1) % SETTINGS_TABS.len();
    }

    fn goto_settings_tab(&mut self, idx: usize) {
        if idx < SETTINGS_TABS.len() {
            self.settings_tab = idx;
        }
    }

    /// Moves the cursor within whichever settings tab is active.
    fn settings_move_next(&mut self) {
        match self.settings_tab {
            0 => self.settings_next(),
            _ => self.browser_next(),
        }
    }

    fn settings_move_prev(&mut self) {
        match self.settings_tab {
            0 => self.settings_prev(),
            _ => self.browser_prev(),
        }
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
        if self.settings_tab != 0 {
            return;
        }
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
    /// applies the chosen browser, persists both to disk, and closes the
    /// settings dialog.
    fn apply_settings(&mut self, editions: Vec<String>) {
        let browser = match self.settings_browsers.get(self.browser_cursor) {
            Some(name) if self.browser_cursor != 0 => Some(name.clone()),
            _ => None,
        };
        self.browser = browser.clone();

        let (groups, status) = fetch::fetch_all(&self.client, &editions, self.start_date, self.max_back, None);
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

        if let Err(e) = config::save(&config::Config { editions, browser }) {
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
    browser: Option<String>,
) -> Result<()> {
    let mut terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture)?;
    let mut app = App::new(client, start_date, max_back, groups, status, browser);
    let result = event_loop(&mut terminal, &mut app);
    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn event_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()>
where
    <B as Backend>::Error: std::error::Error + Send + Sync + 'static,
{
    loop {
        terminal.draw(|f| draw(f, app))?;

        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    if app.settings_open {
                        match key.code {
                            KeyCode::Esc => app.settings_cancel(),
                            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => app.settings_tab_next(),
                            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => app.settings_tab_prev(),
                            KeyCode::Char('j') | KeyCode::Down => app.settings_move_next(),
                            KeyCode::Char('k') | KeyCode::Up => app.settings_move_prev(),
                            KeyCode::Char(' ') => app.settings_toggle(),
                            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                                let idx = c.to_digit(10).unwrap() as usize - 1;
                                app.goto_settings_tab(idx);
                            }
                            KeyCode::Enter => {
                                let chosen = app.settings_chosen();
                                if chosen.is_empty() {
                                    app.message = Some("Mindestens eine Edition auswählen.".to_string());
                                } else {
                                    app.message = Some("Lade neue Editionen…".to_string());
                                    terminal.draw(|f| draw(f, app))?;
                                    app.apply_settings(chosen);
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
                Event::Mouse(mouse) => {
                    let size = terminal.size()?;
                    let area = Rect::new(0, 0, size.width, size.height);
                    handle_mouse(app, mouse, area);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn handle_mouse(app: &mut App, mouse: crossterm::event::MouseEvent, area: Rect) {
    if app.settings_open {
        let (tabs_area, content_area, _) = settings_layout(area);
        match mouse.kind {
            MouseEventKind::ScrollDown => app.settings_move_next(),
            MouseEventKind::ScrollUp => app.settings_move_prev(),
            MouseEventKind::Down(MouseButton::Left) => {
                if point_in(tabs_area, mouse.column, mouse.row) {
                    if let Some(idx) = settings_tab_index_at(tabs_area, mouse.column, mouse.row) {
                        app.goto_settings_tab(idx);
                    }
                } else if let Some(idx) = row_index_in_list(content_area, mouse.column, mouse.row, 0) {
                    match app.settings_tab {
                        0 => {
                            if idx < config::ALL_EDITIONS.len() {
                                app.settings_cursor = idx;
                                app.settings_toggle();
                            }
                        }
                        _ => {
                            if idx < app.settings_browsers.len() {
                                app.browser_cursor = idx;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        return;
    }

    let (tabs_area, list_area, detail_area, _status_area) = main_layout(area);

    match mouse.kind {
        MouseEventKind::ScrollDown => {
            app.next();
            app.message = None;
        }
        MouseEventKind::ScrollUp => {
            app.prev();
            app.message = None;
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if point_in(tabs_area, mouse.column, mouse.row) {
                if let Some(idx) = tab_index_at(app, tabs_area, mouse.column, mouse.row) {
                    app.goto_tab(idx);
                }
            } else if point_in(list_area, mouse.column, mouse.row) {
                let offset = app.current().state.offset();
                if let Some(idx) = row_index_in_list(list_area, mouse.column, mouse.row, offset) {
                    if idx < app.current().stories.len() {
                        if app.current().state.selected() == Some(idx) {
                            app.open_selected();
                        } else {
                            app.current_mut().state.select(Some(idx));
                            app.message = None;
                        }
                    }
                }
            } else if point_in(detail_area, mouse.column, mouse.row) {
                app.open_selected();
            }
        }
        _ => {}
    }
}

fn point_in(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}

/// Maps a click row to a story index, accounting for the block border and the
/// list's current scroll offset (as tracked by ListState after the last render).
fn row_index_in_list(area: Rect, x: u16, y: u16, offset: usize) -> Option<usize> {
    let inner = inner_area(area);
    if !point_in(inner, x, y) {
        return None;
    }
    Some(offset + (y - inner.y) as usize)
}

fn inner_area(area: Rect) -> Rect {
    Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), area.height.saturating_sub(2))
}

/// Approximates which tab title was clicked by re-computing the same widths
/// the Tabs widget lays out with (" N NAME (count) " + "│" divider).
fn tab_index_at(app: &App, area: Rect, x: u16, y: u16) -> Option<usize> {
    let inner = inner_area(area);
    if !point_in(inner, x, y) {
        return None;
    }
    let mut cursor = inner.x;
    for (i, t) in app.tabs.iter().enumerate() {
        let title = format!(" {} {} ({}) ", i + 1, t.name.to_uppercase(), t.stories.len());
        let width = title.chars().count() as u16;
        if x >= cursor && x < cursor + width {
            return Some(i);
        }
        cursor += width + 1;
    }
    None
}

/// Approximates which settings tab title was clicked, mirroring the layout
/// `draw_settings_tabs` renders (" N Name " + "│" divider).
fn settings_tab_index_at(area: Rect, x: u16, y: u16) -> Option<usize> {
    let inner = inner_area(area);
    if !point_in(inner, x, y) {
        return None;
    }
    let mut cursor = inner.x;
    for (i, name) in SETTINGS_TABS.iter().enumerate() {
        let title = format!(" {} {} ", i + 1, name);
        let width = title.chars().count() as u16;
        if x >= cursor && x < cursor + width {
            return Some(i);
        }
        cursor += width + 1;
    }
    None
}

fn main_layout(area: Rect) -> (Rect, Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(chunks[1]);

    (chunks[0], body[0], body[1], chunks[2])
}

fn settings_layout(area: Rect) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3), Constraint::Length(1)])
        .split(area);
    (chunks[0], chunks[1], chunks[2])
}

fn draw(f: &mut Frame, app: &mut App) {
    if app.settings_open {
        draw_settings(f, f.area(), app);
        return;
    }

    let (tabs_area, list_area, detail_area, status_area) = main_layout(f.area());

    draw_tabs(f, tabs_area, app);
    draw_list(f, list_area, app);
    draw_detail(f, detail_area, app);
    draw_status(f, status_area, app);
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
                .title(" TLDR — 1-9/Tab/←→/Klick wechseln, j/k/Scroll/Klick bewegen, Enter/o/Klick öffnen, s Einstellungen, q beenden "),
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
    let (tabs_area, content_area, footer_area) = settings_layout(area);

    draw_settings_tabs(f, tabs_area, app);
    match app.settings_tab {
        0 => draw_settings_editions(f, content_area, app),
        _ => draw_settings_browser(f, content_area, app),
    }

    let hint = match app.settings_tab {
        0 => "j/k/Scroll bewegen, Space/Klick togglen",
        _ => "j/k/Scroll/Klick auswählen",
    };
    let footer_text = app
        .message
        .clone()
        .unwrap_or_else(|| format!("1/2/Tab/←→ Reiter wechseln, {hint}, Enter übernehmen & speichern, Esc abbrechen"));
    let footer = Paragraph::new(footer_text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, footer_area);
}

fn draw_settings_tabs(f: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<Line> =
        SETTINGS_TABS.iter().enumerate().map(|(i, name)| Line::from(format!(" {} {} ", i + 1, name))).collect();

    let tabs = Tabs::new(titles)
        .select(app.settings_tab)
        .block(Block::default().borders(Borders::ALL).title(" Einstellungen "))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Black).bg(Color::Cyan))
        .divider("│");

    f.render_widget(tabs, area);
}

fn draw_settings_editions(f: &mut Frame, area: Rect, app: &App) {
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

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Editionen auswählen "))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray))
        .highlight_symbol("➤ ");

    f.render_stateful_widget(list, area, &mut list_state);
}

fn draw_settings_browser(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> =
        app.settings_browsers.iter().map(|name| ListItem::new(Line::from(name.as_str()))).collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.browser_cursor));

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Browser zum Öffnen von Links "))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray))
        .highlight_symbol("➤ ");

    f.render_stateful_widget(list, area, &mut list_state);
}
