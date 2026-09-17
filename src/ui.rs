use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};

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
}

impl App {
    fn new(groups: Vec<(String, Vec<Story>)>, status: String) -> Self {
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
        App { tabs, active: 0, status, message: None }
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
}

pub fn run(groups: Vec<(String, Vec<Story>)>, status: String) -> Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new(groups, status);
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
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
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
                .title(" TLDR — 1-9 / Tab / ←→ wechseln, j/k bewegen, Enter/o öffnen, q beenden "),
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
