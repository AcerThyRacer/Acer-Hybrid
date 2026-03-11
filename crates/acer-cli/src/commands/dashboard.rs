//! Dashboard command - inspect Acer Hybrid in a terminal UI

use crate::{
    plugins::{load_plugins, PluginManifest},
    runtime::{build_router, policy_engine, trace_store},
};
use acer_core::{AcerConfig, Model, RunRecord};
use acer_trace::UsageStats;
use anyhow::Result;
use chrono::{Duration, Utc};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::io::{self, stdout};
use std::time::{Duration as StdDuration, Instant};

const SPLASH_MS: u64 = 1300;
const ART_LINES: [&str; 5] = [
    "   ___  ________________  __  ____  _____  ___  ________",
    "  / _ |/ ___/ __/ _ \\___// / / /\\ \\/ / _ )/ _ \\/  _/ __ \\",
    " / __ / /__/ _// , _/___/ /_/ /  \\  / _  / , _// // /_/ /",
    "/_/ |_\\___/___/_/|_|    \\____/   /_/____/_/|_/___/\\____/ ",
    "                                                         ",
];

const MENU: [MenuItem; 7] = [
    MenuItem::new(
        "Command Center",
        "Live overview of the local-first control plane.",
    ),
    MenuItem::new(
        "Recent Runs",
        "Inspect the newest traces, latency, and failures.",
    ),
    MenuItem::new(
        "Models",
        "Browse the models currently reachable from the router.",
    ),
    MenuItem::new(
        "Providers",
        "See provider health and which backends are online.",
    ),
    MenuItem::new(
        "Policy",
        "Review the active policy profile and enforcement rules.",
    ),
    MenuItem::new(
        "Plugins",
        "Check installed provider and workflow manifests.",
    ),
    MenuItem::new(
        "Quick Start",
        "Use arrow keys or the mouse to jump into the right workflow.",
    ),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum UiPhase {
    Splash,
    Main,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Theme {
    Rave,
    Matrix,
    Cyberpunk,
    Monochrome,
}

impl Theme {
    fn next(self) -> Self {
        match self {
            Theme::Rave => Theme::Matrix,
            Theme::Matrix => Theme::Cyberpunk,
            Theme::Cyberpunk => Theme::Monochrome,
            Theme::Monochrome => Theme::Rave,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Theme::Rave => "Rave",
            Theme::Matrix => "Matrix",
            Theme::Cyberpunk => "Cyberpunk",
            Theme::Monochrome => "Monochrome",
        }
    }

    fn primary(self) -> Color {
        match self {
            Theme::Rave => Color::Magenta,
            Theme::Matrix => Color::LightGreen,
            Theme::Cyberpunk => Color::Cyan,
            Theme::Monochrome => Color::White,
        }
    }

    fn secondary(self) -> Color {
        match self {
            Theme::Rave => Color::Cyan,
            Theme::Matrix => Color::Green,
            Theme::Cyberpunk => Color::Magenta,
            Theme::Monochrome => Color::Gray,
        }
    }

    fn highlight_bg(self) -> Color {
        match self {
            Theme::Rave => Color::Rgb(64, 23, 64),
            Theme::Matrix => Color::Rgb(20, 64, 20),
            Theme::Cyberpunk => Color::Rgb(23, 41, 64),
            Theme::Monochrome => Color::Rgb(40, 40, 40),
        }
    }

    fn highlight_fg(self) -> Color {
        match self {
            Theme::Rave => Color::Yellow,
            Theme::Matrix => Color::LightGreen,
            Theme::Cyberpunk => Color::LightMagenta,
            Theme::Monochrome => Color::White,
        }
    }

    fn text(self) -> Color {
        match self {
            Theme::Rave => Color::White,
            Theme::Matrix => Color::LightGreen,
            Theme::Cyberpunk => Color::LightCyan,
            Theme::Monochrome => Color::White,
        }
    }

    fn muted(self) -> Color {
        match self {
            Theme::Rave => Color::DarkGray,
            Theme::Matrix => Color::DarkGray,
            Theme::Cyberpunk => Color::DarkGray,
            Theme::Monochrome => Color::DarkGray,
        }
    }

    fn success(self) -> Color {
        match self {
            Theme::Rave => Color::LightGreen,
            Theme::Matrix => Color::LightGreen,
            Theme::Cyberpunk => Color::Green,
            Theme::Monochrome => Color::White,
        }
    }

    fn error(self) -> Color {
        match self {
            Theme::Rave => Color::LightRed,
            Theme::Matrix => Color::LightGreen,
            Theme::Cyberpunk => Color::LightRed,
            Theme::Monochrome => Color::Gray,
        }
    }
}

#[derive(Default)]
struct DashboardData {
    stats: UsageStats,
    runs: Vec<RunRecord>,
    models: Vec<Model>,
    provider_health: Vec<(String, bool)>,
    plugins: Vec<PluginManifest>,
    policy_lines: Vec<String>,
    issues: Vec<String>,
    last_refresh: Option<String>,
}

struct App {
    phase: UiPhase,
    started_at: Instant,
    refresh_every: StdDuration,
    next_refresh_at: Instant,
    selected: usize,
    menu_state: ListState,
    data: DashboardData,
    status: String,
    last_area: Rect,
    theme: Theme,
}

#[derive(Clone, Copy)]
struct MenuItem {
    title: &'static str,
    description: &'static str,
}

impl MenuItem {
    const fn new(title: &'static str, description: &'static str) -> Self {
        Self { title, description }
    }
}

pub async fn execute(refresh_ms: u64) -> Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let _terminal_guard = TerminalGuard;

    let backend = ratatui::backend::CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(refresh_ms);

    loop {
        if app.phase == UiPhase::Splash
            && app.started_at.elapsed() >= StdDuration::from_millis(SPLASH_MS)
        {
            app.phase = UiPhase::Main;
            app.refresh().await;
        }

        if app.phase == UiPhase::Main && Instant::now() >= app.next_refresh_at {
            app.refresh().await;
        }

        terminal.draw(|frame| {
            app.last_area = frame.area();
            match app.phase {
                UiPhase::Splash => render_splash(frame, &app),
                UiPhase::Main => render_dashboard(frame, &mut app),
            }
        })?;

        if !event::poll(StdDuration::from_millis(50))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if handle_key_event(&mut app, key.code).await? {
                    break;
                }
            }
            Event::Mouse(mouse) => {
                if handle_mouse_event(&mut app, mouse).await? {
                    break;
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }

    Ok(())
}

impl App {
    fn new(refresh_ms: u64) -> Self {
        let mut menu_state = ListState::default();
        menu_state.select(Some(0));

        Self {
            phase: UiPhase::Splash,
            started_at: Instant::now(),
            refresh_every: StdDuration::from_millis(refresh_ms.max(250)),
            next_refresh_at: Instant::now(),
            selected: 0,
            menu_state,
            data: DashboardData::default(),
            status: "Booting control surface...".to_string(),
            last_area: Rect::default(),
            theme: Theme::Rave,
        }
    }

    async fn refresh(&mut self) {
        self.status = "Refreshing traces, models, providers, and policy...".to_string();

        let mut next = DashboardData::default();
        let config = match AcerConfig::load() {
            Ok(config) => config,
            Err(error) => {
                self.data.issues = vec![format!("Failed to load config: {}", error)];
                self.status = "Config load failed.".to_string();
                self.next_refresh_at = Instant::now() + self.refresh_every;
                return;
            }
        };

        match trace_store(&config).await {
            Ok(store) => {
                match store.get_stats(Utc::now() - Duration::hours(24)).await {
                    Ok(stats) => next.stats = stats,
                    Err(error) => next.issues.push(format!("Stats unavailable: {}", error)),
                }
                match store.list_runs(12).await {
                    Ok(runs) => next.runs = runs,
                    Err(error) => next
                        .issues
                        .push(format!("Recent runs unavailable: {}", error)),
                }
            }
            Err(error) => next
                .issues
                .push(format!("Trace store unavailable: {}", error)),
        }

        match load_plugins() {
            Ok(plugins) => next.plugins = plugins,
            Err(error) => next.issues.push(format!("Plugins unavailable: {}", error)),
        }

        match build_router(&config, None, false).await {
            Ok(router) => {
                next.provider_health = router
                    .check_availability()
                    .await
                    .into_iter()
                    .collect::<Vec<_>>();
                next.provider_health
                    .sort_by(|left, right| left.0.cmp(&right.0));

                match router.list_all_models().await {
                    Ok(mut models) => {
                        models.sort_by(|left, right| left.id.cmp(&right.id));
                        next.models = models;
                    }
                    Err(error) => next.issues.push(format!("Models unavailable: {}", error)),
                }
            }
            Err(error) => next.issues.push(format!("Router unavailable: {}", error)),
        }

        let policy = policy_engine(&config, None);
        let rules = policy.current_rules();
        next.policy_lines = vec![
            format!(
                "Active profile: {}",
                config
                    .policy
                    .active_profile
                    .clone()
                    .unwrap_or_else(|| "default".to_string())
            ),
            format!("Max cost per request: ${:.4}", rules.max_cost_usd),
            format!("Remote providers allowed: {}", yes_no(rules.allow_remote)),
            format!("PII redaction enabled: {}", yes_no(rules.redact_pii)),
            format!(
                "Require confirmation: {}",
                yes_no(rules.require_confirmation)
            ),
            format!(
                "Allowed tools: {}",
                if rules.allow_tools.is_empty() {
                    "any".to_string()
                } else {
                    rules.allow_tools.join(", ")
                }
            ),
            format!(
                "Blocked patterns: {}",
                if rules.block_patterns.is_empty() {
                    "none".to_string()
                } else {
                    rules.block_patterns.join(", ")
                }
            ),
        ];

        next.last_refresh = Some(Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string());

        self.data = next;
        self.status = format!(
            "Ready. {} issues, {} runs, {} models, {} plugins.",
            self.data.issues.len(),
            self.data.runs.len(),
            self.data.models.len(),
            self.data.plugins.len()
        );
        self.next_refresh_at = Instant::now() + self.refresh_every;
    }

    fn set_selected(&mut self, index: usize) {
        self.selected = index.min(MENU.len().saturating_sub(1));
        self.menu_state.select(Some(self.selected));
        self.status = format!("Selected {}.", MENU[self.selected].title);
    }

    fn next(&mut self) {
        self.set_selected((self.selected + 1) % MENU.len());
    }

    fn previous(&mut self) {
        let next = if self.selected == 0 {
            MENU.len() - 1
        } else {
            self.selected - 1
        };
        self.set_selected(next);
    }
}

async fn handle_key_event(app: &mut App, code: KeyCode) -> Result<bool> {
    if app.phase == UiPhase::Splash {
        app.phase = UiPhase::Main;
        app.refresh().await;
        return Ok(false);
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
        KeyCode::Up | KeyCode::Char('k') => app.previous(),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => app.next(),
        KeyCode::Home => app.set_selected(0),
        KeyCode::End => app.set_selected(MENU.len() - 1),
        KeyCode::Enter | KeyCode::Char('r') => app.refresh().await,
        KeyCode::Char('t') | KeyCode::Char('c') => {
            app.theme = app.theme.next();
            app.status = format!("Theme changed to {}.", app.theme.name());
        }
        _ => {}
    }

    Ok(false)
}

async fn handle_mouse_event(app: &mut App, mouse: crossterm::event::MouseEvent) -> Result<bool> {
    if app.phase == UiPhase::Splash {
        app.phase = UiPhase::Main;
        app.refresh().await;
        return Ok(false);
    }

    match mouse.kind {
        MouseEventKind::ScrollDown => app.next(),
        MouseEventKind::ScrollUp => app.previous(),
        MouseEventKind::Down(MouseButton::Left) => {
            let layout = layout_map(app.last_area);
            if let Some(index) = menu_hit_test(layout.menu, mouse.column, mouse.row) {
                app.set_selected(index);
            } else if contains_point(layout.refresh_button, mouse.column, mouse.row) {
                app.refresh().await;
            } else if contains_point(layout.quit_button, mouse.column, mouse.row) {
                return Ok(true);
            }
        }
        _ => {}
    }

    Ok(false)
}

fn render_splash(frame: &mut ratatui::Frame<'_>, app: &App) {
    let area = frame.area();
    let elapsed_ms = app.started_at.elapsed().as_millis() as u64;
    let progress = (elapsed_ms as f32 / SPLASH_MS as f32).clamp(0.0, 1.0);
    let line_progress = progress * ART_LINES.len() as f32;
    let full_lines = line_progress.floor() as usize;
    let partial = ((line_progress.fract()) * ART_LINES[0].len() as f32) as usize;

    let mut lines = Vec::new();
    for (index, line) in ART_LINES.iter().enumerate() {
        let rendered = if index < full_lines {
            (*line).to_string()
        } else if index == full_lines {
            line.chars().take(partial).collect()
        } else {
            String::new()
        };
        lines.push(Line::from(Span::styled(
            rendered,
            Style::default()
                .fg(app.theme.primary())
                .add_modifier(Modifier::BOLD),
        )));
    }

    let spinner = ["[=   ]", "[==  ]", "[=== ]", "[ ===]", "[  ==]", "[   =]"];
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        spinner[((elapsed_ms / 90) as usize) % spinner.len()],
        Style::default().fg(app.theme.secondary()),
    )));
    lines.push(Line::from(Span::styled(
        "Launching the local-first control surface. Press any key to skip.",
        Style::default().fg(app.theme.muted()),
    )));

    let splash = Paragraph::new(lines).alignment(Alignment::Center).block(
        Block::default()
            .title(Span::styled(" ACER-HYBRID ", Style::default().fg(app.theme.primary())))
            .borders(Borders::NONE),
    );

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Length(10),
            Constraint::Percentage(20),
        ])
        .split(area);

    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(8),
            Constraint::Min(40),
            Constraint::Percentage(8),
        ])
        .split(layout[1]);

    frame.render_widget(splash, inner[1]);
}

fn render_dashboard(frame: &mut ratatui::Frame<'_>, app: &mut App) {
    let layout = layout_map(frame.area());

    render_header(frame, layout.header, app);
    render_menu(frame, layout.menu, app);
    render_detail(frame, layout.detail, app);
    render_footer(
        frame,
        layout.footer,
        layout.refresh_button,
        layout.quit_button,
        app,
    );
}

struct LayoutMap {
    header: Rect,
    menu: Rect,
    detail: Rect,
    footer: Rect,
    refresh_button: Rect,
    quit_button: Rect,
}

fn layout_map(area: Rect) -> LayoutMap {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(area);
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(34), Constraint::Min(30)])
        .split(vertical[1]);

    let footer_inner = footer_inner(vertical[2]);
    LayoutMap {
        header: vertical[0],
        menu: main[0],
        detail: main[1],
        footer: vertical[2],
        refresh_button: footer_inner[1],
        quit_button: footer_inner[2],
    }
}

fn footer_inner(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(20),
            Constraint::Length(12),
            Constraint::Length(10),
        ])
        .split(area)
        .to_vec()
}

fn render_header(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let spinner = ['|', '/', '-', '\\'];
    let pulse = spinner[((app.started_at.elapsed().as_millis() / 120) as usize) % spinner.len()];
    let art = if area.width >= 72 {
        vec![
            Line::from(Span::styled(
                "   ___  ________________  __  ____  _____  ___  ________",
                Style::default()
                    .fg(app.theme.primary())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  / _ |/ ___/ __/ _ \\___// / / /\\ \\/ / _ )/ _ \\/  _/ __ \\",
                Style::default().fg(app.theme.primary()),
            )),
            Line::from(Span::styled(
                " / __ / /__/ _// , _/___/ /_/ /  \\  / _  / , _// // /_/ /",
                Style::default().fg(app.theme.primary()),
            )),
            Line::from(Span::styled(
                "/_/ |_\\___/___/_/|_|    \\____/   /_/____/_/|_/___/\\____/ ",
                Style::default().fg(app.theme.primary()),
            )),
            Line::from(Span::styled(
                "                                                         ",
                Style::default().fg(app.theme.primary()),
            )),
            Line::from(Span::styled(
                format!(
                    " [{}] local-first control surface  |  last refresh: {}",
                    pulse,
                    app.data
                        .last_refresh
                        .clone()
                        .unwrap_or_else(|| "not yet".to_string())
                ),
                Style::default().fg(app.theme.secondary()),
            )),
        ]
    } else {
        vec![
            Line::from(Span::styled(
                "ACER-HYBRID",
                Style::default()
                    .fg(app.theme.primary())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("[{}] local-first control surface", pulse),
                Style::default().fg(app.theme.secondary()),
            )),
        ]
    };

    let widget = Paragraph::new(art).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(app.theme.muted())),
    );
    frame.render_widget(widget, area);
}

fn render_menu(frame: &mut ratatui::Frame<'_>, area: Rect, app: &mut App) {
    let items = MENU
        .iter()
        .map(|item| {
            ListItem::new(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(item.title, Style::default().fg(app.theme.text())),
            ]))
        })
        .collect::<Vec<_>>();

    let menu = List::new(items)
        .block(
            Block::default()
                .title(Span::styled(" MENU ", Style::default().fg(app.theme.primary())))
                .borders(Borders::NONE),
        )
        .highlight_style(
            Style::default()
                .bg(app.theme.highlight_bg())
                .fg(app.theme.highlight_fg())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(menu, area, &mut app.menu_state);
}

fn render_detail(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let lines = match app.selected {
        0 => detail_command_center(app),
        1 => detail_recent_runs(app),
        2 => detail_models(app),
        3 => detail_providers(app),
        4 => detail_policy(app),
        5 => detail_plugins(app),
        _ => detail_quick_start(app),
    };

    let title = format!(" {} ", MENU[app.selected].title);
    let widget = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(Span::styled(title, Style::default().fg(app.theme.primary())))
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(app.theme.muted())),
        );
    frame.render_widget(widget, area);
}

fn render_footer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    refresh_button: Rect,
    quit_button: Rect,
    app: &App,
) {
    let sections = footer_inner(area);
    let status = Paragraph::new(Line::from(vec![
        Span::styled("STATUS: ", Style::default().fg(app.theme.primary())),
        Span::styled(app.status.clone(), Style::default().fg(app.theme.text())),
        Span::styled(
            "  |  ↑/↓: navigate  |  r: refresh  |  t: theme  |  q: quit",
            Style::default().fg(app.theme.muted()),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(app.theme.muted())),
    );
    frame.render_widget(status, sections[0]);

    frame.render_widget(button("REFRESH", app.theme.success(), app), refresh_button);
    frame.render_widget(button("QUIT", app.theme.error(), app), quit_button);
}

fn button(label: &str, color: Color, app: &App) -> Paragraph<'static> {
    Paragraph::new(Line::from(Span::styled(
        format!(" {}", label),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::TOP | Borders::LEFT)
            .border_style(Style::default().fg(app.theme.muted())),
    )
}

fn detail_command_center(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            MENU[0].description,
            Style::default().fg(app.theme.muted()),
        )),
        Line::from(""),
        metric_line("Requests (24h)", app.data.stats.total_requests.to_string(), app.theme),
        metric_line(
            "Success / Fail",
            format!(
                "{} / {}",
                app.data.stats.successful_requests, app.data.stats.failed_requests
            ),
            app.theme
        ),
        metric_line("Tokens (24h)", app.data.stats.total_tokens.to_string(), app.theme),
        metric_line(
            "Estimated cost",
            format!("${:.4}", app.data.stats.total_cost_usd),
            app.theme
        ),
        metric_line(
            "Avg latency",
            format!("{:.0} ms", app.data.stats.avg_latency_ms),
            app.theme
        ),
        metric_line("Models loaded", app.data.models.len().to_string(), app.theme),
        metric_line("Plugins loaded", app.data.plugins.len().to_string(), app.theme),
        Line::from(""),
        Line::from(Span::styled(
            "Open the palette with arrows or click a panel on the left.",
            Style::default().fg(app.theme.secondary()),
        )),
    ];

    if !app.data.issues.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Warnings",
            Style::default().fg(app.theme.error()).add_modifier(Modifier::BOLD),
        )));
        for issue in app.data.issues.iter().take(4) {
            lines.push(Line::from(format!("- {}", issue)));
        }
    }

    lines
}

fn detail_recent_runs(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            MENU[1].description,
            Style::default().fg(app.theme.muted()),
        )),
        Line::from(""),
    ];

    if app.data.runs.is_empty() {
        lines.push(Line::from("No runs recorded yet."));
        return lines;
    }

    for run in app.data.runs.iter().take(10) {
        let state = if run.success { "ok " } else { "err" };
        lines.push(Line::from(format!(
            "{}  {}  {:<10} {:<24} {:>5} ms",
            run.timestamp.format("%H:%M:%S"),
            state,
            run.provider,
            truncate(&run.model, 24),
            run.latency_ms
        )));
        if let Some(error) = &run.error {
            lines.push(Line::from(format!("    error: {}", truncate(error, 72))));
        }
    }

    lines
}

fn detail_models(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            MENU[2].description,
            Style::default().fg(app.theme.muted()),
        )),
        Line::from(""),
    ];

    if app.data.models.is_empty() {
        lines.push(Line::from("No models are currently available."));
        return lines;
    }

    for model in app.data.models.iter().take(14) {
        let locality = if model.is_local { "local " } else { "remote" };
        let cost = model
            .cost_per_1k_tokens
            .map(|value| format!("${:.4}/1k", value))
            .unwrap_or_else(|| "n/a".to_string());
        lines.push(Line::from(format!(
            "{:<10} {:<6} {:<16} {}",
            model.provider,
            locality,
            truncate(&model.id, 16),
            cost
        )));
    }

    lines
}

fn detail_providers(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            MENU[3].description,
            Style::default().fg(app.theme.muted()),
        )),
        Line::from(""),
    ];

    if app.data.provider_health.is_empty() {
        lines.push(Line::from("No providers registered."));
        return lines;
    }

    for (name, healthy) in &app.data.provider_health {
        lines.push(Line::from(format!(
            "{:<16} {}",
            name,
            if *healthy { "online" } else { "offline" }
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Model counts by provider:"));
    for (provider, stats) in &app.data.stats.by_provider {
        lines.push(Line::from(format!(
            "- {:<12} requests={} tokens={} cost=${:.4}",
            provider, stats.requests, stats.tokens, stats.cost_usd
        )));
    }

    lines
}

fn detail_policy(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            MENU[4].description,
            Style::default().fg(app.theme.muted()),
        )),
        Line::from(""),
    ];

    if app.data.policy_lines.is_empty() {
        lines.push(Line::from("No policy data loaded."));
        return lines;
    }

    for line in &app.data.policy_lines {
        lines.push(Line::from(line.clone()));
    }

    lines
}

fn detail_plugins(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            MENU[5].description,
            Style::default().fg(app.theme.muted()),
        )),
        Line::from(""),
    ];

    if app.data.plugins.is_empty() {
        lines.push(Line::from("No plugin manifests installed."));
        return lines;
    }

    for plugin in &app.data.plugins {
        let kind = if plugin.provider.is_some() {
            "provider"
        } else {
            "workflow"
        };
        lines.push(Line::from(format!(
            "{:<16} {:<9} {}",
            truncate(&plugin.name, 16),
            kind,
            plugin
                .description
                .clone()
                .unwrap_or_else(|| "no description".to_string())
        )));
    }

    lines
}

fn detail_quick_start(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            MENU[6].description,
            Style::default().fg(app.theme.muted()),
        )),
        Line::from(""),
        Line::from("Primary shortcuts"),
        Line::from("  acer            launch the dashboard from any directory"),
        Line::from("  hybrid          same dashboard via the alias"),
        Line::from("  acer ask \"...\"  run a model request"),
        Line::from("  acer models     inspect available models"),
        Line::from("  acer gateway    start the OpenAI-compatible gateway"),
        Line::from("  acer doctor     run a health check"),
        Line::from(""),
        Line::from("Dashboard controls"),
        Line::from("  Up/Down, j/k, mouse wheel  change the selected panel"),
        Line::from("  r or click Refresh         reload live data"),
        Line::from("  q, Esc, or click Quit      exit the dashboard"),
    ];

    if let Some(refreshed) = &app.data.last_refresh {
        lines.push(Line::from(""));
        lines.push(Line::from(format!(
            "Last successful refresh: {}",
            refreshed
        )));
    }

    lines
}

fn metric_line(label: impl Into<String>, value: impl Into<String>, theme: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{:<18}", label.into()),
            Style::default().fg(theme.muted()),
        ),
        Span::styled(value.into(), Style::default().fg(theme.text()).add_modifier(Modifier::BOLD)),
    ])
}

fn menu_hit_test(menu: Rect, x: u16, y: u16) -> Option<usize> {
    if !contains_point(menu, x, y) || y <= menu.y || x <= menu.x {
        return None;
    }

    let index = (y - menu.y - 1) as usize;
    (index < MENU.len()).then_some(index)
}

fn contains_point(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }

    value
        .chars()
        .take(width.saturating_sub(3))
        .collect::<String>()
        + "..."
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}
