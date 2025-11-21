//! Ratatui-based interface wired to Tantivy search (basic).

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{ExecutableCommand, execute};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use std::io;
use std::time::{Duration, Instant};

use crate::search::query::{SearchClient, SearchFilters};
use crate::search::tantivy::index_dir;
use crate::ui::components::widgets::search_bar;

pub fn run_tui() -> Result<()> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let data_dir = crate::default_data_dir();
    let index_path = index_dir(&data_dir)?;
    let search_client = SearchClient::open(&index_path)?;

    let mut query = String::new();
    let mut results: Vec<String> = Vec::new();
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(200);

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(3),
                        Constraint::Min(0),
                        Constraint::Length(1),
                    ]
                    .as_ref(),
                )
                .split(f.area());

            // search bar
            let sb = search_bar(&query);
            f.render_widget(sb, chunks[0]);

            // results
            let items: Vec<ListItem> = if results.is_empty() {
                vec![ListItem::new("(results will appear here)")]
            } else {
                results.iter().map(|r| ListItem::new(r.clone())).collect()
            };
            let list =
                List::new(items).block(Block::default().title("Results").borders(Borders::ALL));
            f.render_widget(list, chunks[1]);

            let footer = Paragraph::new("q/esc quit, type to search");
            f.render_widget(footer, chunks[2]);
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_millis(0));

        if crossterm::event::poll(timeout)?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char(c) => query.push(c),
                KeyCode::Backspace => {
                    query.pop();
                }
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate {
            if let Some(client) = &search_client {
                if !query.is_empty() {
                    if let Ok(hits) = client.search(&query, SearchFilters::default(), 10) {
                        results = hits
                            .into_iter()
                            .map(|h| format!("{:.2} {} — {}", h.score, h.title, h.source_path))
                            .collect();
                    }
                } else {
                    results.clear();
                }
            }
            last_tick = Instant::now();
        }
    }

    teardown_terminal()
}

fn teardown_terminal() -> Result<()> {
    let mut stdout = io::stdout();
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;
    Ok(())
}
