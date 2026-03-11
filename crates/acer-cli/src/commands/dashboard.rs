//! Dashboard command - inspect Acer Hybrid in a terminal UI

use crate::runtime::trace_store;
use acer_core::AcerConfig;
use anyhow::Result;
use chrono::{Duration, Utc};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::io::{self, stdout};
use std::time::Duration as StdDuration;

pub async fn execute(refresh_ms: u64) -> Result<()> {
    let config = AcerConfig::load()?;
    let store = trace_store(&config).await?;

    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let _terminal_guard = TerminalGuard;
    let backend = ratatui::backend::CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let refresh = StdDuration::from_millis(refresh_ms);
    loop {
        let stats = store.get_stats(Utc::now() - Duration::hours(24)).await?;
        let runs = store.list_runs(10).await?;

        terminal.draw(|frame| {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(5),
                    Constraint::Length(7),
                    Constraint::Min(10),
                ])
                .split(frame.area());

            let header = Paragraph::new(vec![
                Line::from(Span::styled(
                    "Acer-Hybrid Dashboard",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(format!(
                    "24h requests: {}  success: {}  failures: {}",
                    stats.total_requests, stats.successful_requests, stats.failed_requests
                )),
                Line::from(format!(
                    "24h tokens: {}  cost: ${:.4}  avg latency: {:.0}ms",
                    stats.total_tokens, stats.total_cost_usd, stats.avg_latency_ms
                )),
            ])
            .block(Block::default().title("Overview").borders(Borders::ALL));
            frame.render_widget(header, layout[0]);

            let providers = stats
                .by_provider
                .iter()
                .map(|(provider, data)| {
                    ListItem::new(format!(
                        "{}  requests={} tokens={} cost=${:.4}",
                        provider, data.requests, data.tokens, data.cost_usd
                    ))
                })
                .collect::<Vec<_>>();
            frame.render_widget(
                List::new(providers)
                    .block(Block::default().title("Providers").borders(Borders::ALL)),
                layout[1],
            );

            let recent = runs
                .into_iter()
                .map(|run| {
                    let status = if run.success { "ok" } else { "err" };
                    ListItem::new(format!(
                        "{} {} {} {} {:.0}ms",
                        run.timestamp.format("%H:%M:%S"),
                        status,
                        run.provider,
                        run.model,
                        run.latency_ms as f64
                    ))
                })
                .collect::<Vec<_>>();
            frame.render_widget(
                List::new(recent).block(
                    Block::default()
                        .title("Recent Runs (q to quit)")
                        .borders(Borders::ALL),
                ),
                layout[2],
            );
        })?;

        if event::poll(refresh)? {
            if let Event::Key(key) = event::read()? {
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    break;
                }
            }
        }
    }

    Ok(())
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}
