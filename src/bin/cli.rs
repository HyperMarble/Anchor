//! Anchor CLI - Code intelligence for AI agents.
//!
//! Usage:
//!   anchor overview              # Codebase overview
//!   anchor search <query>        # Search symbols/files
//!   anchor context <query>       # Get full context
//!   anchor deps <symbol>         # Dependencies
//!   anchor stats                 # Graph statistics
//!   anchor build                 # Rebuild graph (with TUI)
//!   anchor build --no-tui        # Rebuild graph (CLI only)

use anchor::{build_graph, get_context, graph_search, anchor_dependencies, anchor_stats, CodeGraph};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

// TUI imports
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(name = "anchor")]
#[command(about = "Anchor - Code intelligence for AI agents", long_about = None)]
struct Cli {
    /// Project root directory (default: current directory)
    #[arg(short, long, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show codebase overview - files, key symbols, entry points
    Overview {
        /// Disable TUI visualization (use plain CLI output)
        #[arg(long)]
        no_tui: bool,
    },

    /// Search for symbols or files
    Search {
        /// Query string (symbol name or file path)
        query: String,

        /// How many hops to traverse in the graph
        #[arg(short, long, default_value = "1")]
        depth: usize,

        /// Disable TUI visualization (use plain CLI output)
        #[arg(long)]
        no_tui: bool,
    },

    /// Get full context for a symbol (code + dependencies + dependents)
    Context {
        /// Symbol name or file path
        query: String,

        /// Intent: find, understand, modify, refactor, overview
        #[arg(short, long, default_value = "understand")]
        intent: String,

        /// Disable TUI visualization (use plain CLI output)
        #[arg(long)]
        no_tui: bool,
    },

    /// Show what depends on a symbol and what it depends on
    Deps {
        /// Symbol name
        symbol: String,

        /// Disable TUI visualization (use plain CLI output)
        #[arg(long)]
        no_tui: bool,
    },

    /// Show graph statistics
    Stats {
        /// Disable TUI visualization (use plain CLI output)
        #[arg(long)]
        no_tui: bool,
    },

    /// Rebuild the code graph from scratch
    Build {
        /// Disable TUI visualization (use plain CLI output)
        #[arg(long)]
        no_tui: bool,
    },
}

// Color palette for TUI consistency
mod colors {
    use ratatui::style::Color;

    // Brand colors
   pub const PRIMARY: Color = Color::Rgb(52, 211, 153);  // Emerald
    pub const ACCENT: Color = Color::Rgb(16, 185, 129);   // Green
    pub const WARNING: Color = Color::Rgb(234, 179, 8);   // Yellow
    pub const ERROR: Color = Color::Rgb(239, 68, 68);     // Red
    pub const INFO: Color = Color::Rgb(59, 130, 246);     // Blue

    // UI colors
    pub const BG_DARK: Color = Color::Rgb(10, 10, 15);
    pub const BG_BLACK: Color = Color::Black;
    pub const BORDER: Color = Color::Rgb(52, 211, 153);

    // Text colors
    pub const TEXT_BRIGHT: Color = Color::Rgb(226, 232, 240);
    pub const TEXT_NORMAL: Color = Color::Rgb(203, 213, 225);
    pub const TEXT_MUTED: Color = Color::Rgb(148, 163, 184);
    pub const TEXT_DIM: Color = Color::Rgb(100, 116, 139);
    pub const TEXT_FAINT: Color = Color::Rgb(71, 85, 105);
}

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let root = cli.root.canonicalize().unwrap_or(cli.root);
    let cache_path = root.join(".anchor").join("graph.bin");

    match cli.command {
        Commands::Build { no_tui } => {
            if no_tui || !atty::is(atty::Stream::Stdout) {
                // CLI mode
                build_cli_mode(&root, &cache_path)?;
            } else {
                // TUI mode (default)
                build_tui_mode(&root, &cache_path)?;
            }
            return Ok(());
        }
        _ => {}
    }

    // For other commands, load the existing graph
    let graph = if cache_path.exists() {
        CodeGraph::load(&cache_path)?
    } else {
        eprintln!("Building graph (first run)...");
        let graph = build_graph(&root);
        std::fs::create_dir_all(cache_path.parent().unwrap())?;
        graph.save(&cache_path)?;
        graph
    };

    match cli.command {
        Commands::Overview { no_tui } => {
            if no_tui || !atty::is(atty::Stream::Stdout) {
                overview_cli_mode(&graph)?;
            } else {
                overview_tui_mode(&graph)?;
            }
        }

        Commands::Search { query, depth, no_tui } => {
            if no_tui || !atty::is(atty::Stream::Stdout) {
                search_cli_mode(&graph, &query, depth)?;
            } else {
                search_tui_mode(&graph, &query, depth)?;
            }
        }

        Commands::Context { query, intent, no_tui } => {
            if no_tui || !atty::is(atty::Stream::Stdout) {
                context_cli_mode(&graph, &query, &intent)?;
            } else {
                context_tui_mode(&graph, &query, &intent)?;
            }
        }

        Commands::Deps { symbol, no_tui } => {
            if no_tui || !atty::is(atty::Stream::Stdout) {
                deps_cli_mode(&graph, &symbol)?;
            } else {
                deps_tui_mode(&graph, &symbol)?;
            }
        }

        Commands::Stats { no_tui } => {
            if no_tui || !atty::is(atty::Stream::Stdout) {
                stats_cli_mode(&graph)?;
            } else {
                stats_tui_mode(&graph)?;
            }
        }

        Commands::Build { .. } => {
            // Already handled above
        }
    }

    Ok(())
}

// CLI mode build (plain text output)
fn build_cli_mode(root: &PathBuf, cache_path: &PathBuf) -> Result<()> {
    eprintln!("Rebuilding graph...");
    let graph = build_graph(root);
    std::fs::create_dir_all(cache_path.parent().unwrap())?;
    graph.save(cache_path)?;

    let stats = graph.stats();
    println!("✓ Graph built");
    println!("  Files:   {}", stats.file_count);
    println!("  Symbols: {}", stats.symbol_count);
    println!("  Edges:   {}", stats.total_edges);
    Ok(())
}

// CLI mode for overview
fn overview_cli_mode(graph: &CodeGraph) -> Result<()> {
    let stats = graph.stats();
    println!("Anchor - Codebase Overview");
    println!("══════════════════════════");
    println!();
    println!("Files:   {}", stats.file_count);
    println!("Symbols: {}", stats.symbol_count);
    println!("Edges:   {}", stats.total_edges);
    println!();

    // Show top-level structure
    let result = graph_search(graph, "src/", 0);
    if !result.matched_files.is_empty() {
        println!("Structure:");
        for file in result.matched_files.iter().take(15) {
            println!("  {}", file.display());
        }
        if result.matched_files.len() > 15 {
            println!("  ... and {} more", result.matched_files.len() - 15);
        }
    }
    println!();

    // Show entry points (main functions)
    let mains = graph_search(graph, "main", 0);
    if !mains.symbols.is_empty() {
        println!("Entry points:");
        for sym in mains.symbols.iter().filter(|s| s.name == "main") {
            println!("  {} in {}", sym.name, sym.file.display());
        }
    }
    Ok(())
}

// CLI mode for search
fn search_cli_mode(graph: &CodeGraph, query: &str, depth: usize) -> Result<()> {
    let result = graph_search(graph, query, depth);

    if result.symbols.is_empty() && result.matched_files.is_empty() {
        println!("No results for '{}'", query);
        return Ok(());
    }

    println!("Search: '{}' (depth={})", query, depth);
    println!();

    if !result.matched_files.is_empty() {
        println!("Files:");
        for file in &result.matched_files {
            println!("  {}", file.display());
        }
        println!();
    }

    if !result.symbols.is_empty() {
        println!("Symbols:");
        for sym in &result.symbols {
            println!("  {} ({}) - {}:{}", sym.name, sym.kind, sym.file.display(), sym.line);
        }
        println!();
    }

    if !result.connections.is_empty() {
        println!("Connections:");
        for conn in result.connections.iter().take(20) {
            println!("  {} --[{}]-> {}", conn.from, conn.relationship, conn.to);
        }
        if result.connections.len() > 20 {
            println!("  ... and {} more", result.connections.len() - 20);
        }
    }
    Ok(())
}

// CLI mode for context
fn context_cli_mode(graph: &CodeGraph, query: &str, intent: &str) -> Result<()> {
    let result = get_context(graph, query, intent);
    let json = serde_json::to_string_pretty(&result).unwrap_or_default();
    println!("{}", json);
    Ok(())
}

// CLI mode for deps
fn deps_cli_mode(graph: &CodeGraph, symbol: &str) -> Result<()> {
    let result = anchor_dependencies(graph, symbol);
    let json = serde_json::to_string_pretty(&result).unwrap_or_default();
    println!("{}", json);
    Ok(())
}

// CLI mode for stats
fn stats_cli_mode(graph: &CodeGraph) -> Result<()> {
    let result = anchor_stats(graph);
    let json = serde_json::to_string_pretty(&result).unwrap_or_default();
    println!("{}", json);
    Ok(())
}


#[derive(Clone)]
struct BuildProgress {
    complete: bool,
    error: Option<String>,
    stats: Option<(usize, usize, usize)>, // (files, symbols, edges)
}

// TUI mode build (visual interface)
fn build_tui_mode(root: &PathBuf, cache_path: &PathBuf) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let progress = Arc::new(Mutex::new(BuildProgress {
        complete: false,
        error: None,
        stats: None,
    }));

    let mut tui_state = TUIState::new(root.clone());
    
    // Build graph in background thread
    let progress_clone = Arc::clone(&progress);
    let root_clone = root.clone();
    let cache_path_clone = cache_path.clone();
    
    thread::spawn(move || {
        match build_graph_with_progress(&root_clone, &cache_path_clone) {
            Ok(stats) => {
                let mut prog = progress_clone.lock().unwrap();
                prog.stats = Some(stats);
                prog.complete = true;
            }
            Err(e) => {
                let mut prog = progress_clone.lock().unwrap();
                prog.error = Some(e.to_string());
                prog.complete = true;
            }
        }
    });

    let res = run_tui(&mut terminal, &mut tui_state, &progress);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn build_graph_with_progress(root: &PathBuf, cache_path: &PathBuf) -> Result<(usize, usize, usize)> {
    let graph = build_graph(root);
    std::fs::create_dir_all(cache_path.parent().unwrap())?;
    graph.save(cache_path)?;
    
    let stats = graph.stats();
    Ok((stats.file_count, stats.symbol_count, stats.total_edges))
}

struct TUIState {
    _root: PathBuf,
    messages: Vec<String>,
    animation_frame: usize,
    last_update: Instant,
    start_time: Instant,
}

impl TUIState {
    fn new(root: PathBuf) -> Self {
        Self {
            _root: root,
            messages: vec!["Rebuilding graph...".to_string()],
            animation_frame: 0,
            last_update: Instant::now(),
            start_time: Instant::now(),
        }
    }

    fn update(&mut self, progress: &Arc<Mutex<BuildProgress>>) {
        if self.last_update.elapsed() >= Duration::from_millis(100) {
            self.animation_frame = (self.animation_frame + 1) % 8;
            self.last_update = Instant::now();

            // Check if build completed
            let prog = progress.lock().unwrap();
            if prog.complete && self.messages.len() == 1 {
                drop(prog);
                self.finalize_messages(progress);
            }
        }
    }

    fn finalize_messages(&mut self, progress: &Arc<Mutex<BuildProgress>>) {
        let prog = progress.lock().unwrap();
        
        if let Some(error) = &prog.error {
            self.messages.push(format!("✗ Build failed: {}", error));
        } else if let Some((files, symbols, edges)) = prog.stats {
            self.messages.push("✓ Graph built".to_string());
            self.messages.push(format!("  Files:   {}", files));
            self.messages.push(format!("  Symbols: {}", symbols));
            self.messages.push(format!("  Edges:   {}", edges));
        }
    }
}

fn run_tui(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut TUIState,
    progress: &Arc<Mutex<BuildProgress>>,
) -> Result<()> {
    loop {
        terminal.draw(|f| draw_tui(f, state, progress))?;

        // Check for completion
        let prog = progress.lock().unwrap();
        let is_complete = prog.complete;
        let has_error = prog.error.is_some();
        drop(prog);

        if is_complete {
            // After completion, wait for user input or timeout
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') => {
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
            
            // Auto-exit after showing results for a bit
            if state.start_time.elapsed() > Duration::from_secs(10) && !has_error {
                return Ok(());
            }
        } else {
            // During build, just check for quit
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                        return Ok(());
                    }
                }
            }
        }

        state.update(progress);
    }
}

fn draw_tui(f: &mut Frame, state: &TUIState, progress: &Arc<Mutex<BuildProgress>>) {
    let size = f.size();
    
    // Adaptive layout based on terminal size
    let (show_logo, show_info) = if size.height < 18 {
        (false, false)  // Minimal mode
    } else if size.height < 26 {
        (true, false)   // Medium mode
    } else {
        (true, true)    // Full mode
    };

    let mut constraints = vec![];
    if show_logo {
        constraints.push(Constraint::Length(3));  // Reduced from 5 to 3
    }
    constraints.push(Constraint::Min(6));
    if show_info {
        constraints.push(Constraint::Length(10));
    }
    constraints.push(Constraint::Length(1));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(size);

    let mut idx = 0;
    
    if show_logo {
        draw_header(f, chunks[idx], size.width);
        idx += 1;
    }
    
    draw_output(f, chunks[idx], state, progress, size.width);
    idx += 1;
    
    if show_info {
        draw_info(f, chunks[idx], progress);
        idx += 1;
    }
    
    draw_footer(f, chunks[idx], progress);
}

fn draw_header(f: &mut Frame, area: Rect, width: u16) {
    let logo = if width < 70 {
        // Compact version for narrow terminals (2 lines)
        vec![
            Line::from(vec![
                Span::styled(" ⚓ ", Style::default()
                    .fg(Color::Rgb(52, 211, 153))
                    .add_modifier(Modifier::BOLD)),
                Span::styled("ANCHOR", Style::default()
                    .fg(Color::Rgb(52, 211, 153))
                    .add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled("LSP for AI • Zero Tokens", Style::default().fg(Color::Rgb(100, 116, 139))),
            ]),
        ]
    } else {
        // Wider terminal version (2 lines)
        vec![
            Line::from(vec![
                Span::styled(" ⚓ ", Style::default()
                    .fg(Color::Rgb(52, 211, 153))
                    .add_modifier(Modifier::BOLD)),
                Span::styled("ANCHOR", Style::default()
                    .fg(Color::Rgb(52, 211, 153))
                    .add_modifier(Modifier::BOLD)),
                Span::styled("  •  ", Style::default().fg(Color::Rgb(71, 85, 105))),
                Span::styled("LSP for AI Agents", Style::default().fg(Color::Rgb(100, 116, 139))),
                Span::styled("  •  ", Style::default().fg(Color::Rgb(71, 85, 105))),
                Span::styled("Zero Tokens", Style::default().fg(Color::Rgb(16, 185, 129)).add_modifier(Modifier::BOLD)),
            ]),
        ]
    };

    let header = Paragraph::new(logo).style(Style::default().bg(Color::Black));
    f.render_widget(header, area);
}

fn draw_output(f: &mut Frame, area: Rect, state: &TUIState, progress: &Arc<Mutex<BuildProgress>>, _width: u16) {
    let prog = progress.lock().unwrap();
    let is_complete = prog.complete;
    let has_error = prog.error.is_some();
    drop(prog);

    let title = if is_complete {
        if has_error {
            "  anchor build  ✗  "
        } else {
            "  anchor build  ✓  "
        }
    } else {
        "  anchor build  "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(52, 211, 153)))
        .style(Style::default().bg(Color::Rgb(10, 10, 15)))
        .title(Span::styled(title, Style::default()
            .fg(Color::Rgb(52, 211, 153))
            .add_modifier(Modifier::BOLD)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    // Show all messages
    for msg in &state.messages {
        let color = if msg.starts_with('✓') {
            Color::Rgb(52, 211, 153)
        } else if msg.starts_with('✗') {
            Color::Rgb(239, 68, 68)
        } else if msg.starts_with("  ") {
            Color::Rgb(148, 163, 184)
        } else {
            Color::Rgb(203, 213, 225)
        };
        lines.push(Line::from(Span::styled(msg, Style::default().fg(color).add_modifier(Modifier::BOLD))));
    }

    // Show spinner if not complete
    if !is_complete {
        let spinner = match state.animation_frame % 8 {
            0 => "⠋",
            1 => "⠙",
            2 => "⠹",
            3 => "⠸",
            4 => "⠼",
            5 => "⠴",
            6 => "⠦",
            _ => "⠧",
        };
        
        let elapsed = state.start_time.elapsed().as_secs();
        
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(spinner, Style::default().fg(Color::Rgb(234, 179, 8)).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled("Parsing files and building graph", Style::default().fg(Color::Rgb(148, 163, 184))),
            Span::raw("   "),
            Span::styled(format!("{}s", elapsed), Style::default().fg(Color::Rgb(100, 116, 139))),
        ]));
    }

    let output = Paragraph::new(lines)
        .style(Style::default().bg(Color::Rgb(10, 10, 15)))
        .wrap(Wrap { trim: false });

    f.render_widget(output, inner);
}

fn draw_info(f: &mut Frame, area: Rect, progress: &Arc<Mutex<BuildProgress>>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(52, 211, 153)))
        .style(Style::default().bg(Color::Rgb(10, 10, 15)))
        .title(Span::styled("  System Info  ", Style::default()
            .fg(Color::Rgb(52, 211, 153))
            .add_modifier(Modifier::BOLD)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let prog = progress.lock().unwrap();
    let (files, symbols, edges) = prog.stats.unwrap_or((0, 0, 0));
    drop(prog);

    let lines = vec![
        Line::from(vec![
            Span::styled("Engine      ", Style::default().fg(Color::Rgb(100, 116, 139))),
            Span::styled("Rust + Tree-sitter", Style::default().fg(Color::Rgb(226, 232, 240)).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Storage     ", Style::default().fg(Color::Rgb(100, 116, 139))),
            Span::styled("In-Memory Graph (RAM)", Style::default().fg(Color::Rgb(192, 132, 252))),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Token Cost  ", Style::default().fg(Color::Rgb(100, 116, 139))),
            Span::styled("0 tokens ", Style::default().fg(Color::Rgb(52, 211, 153)).add_modifier(Modifier::BOLD)),
            Span::styled("(structure is free!)", Style::default().fg(Color::Rgb(100, 116, 139))),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Graph Stats ", Style::default().fg(Color::Rgb(100, 116, 139))),
            Span::styled(
                format!("{} files • {} symbols • {} edges", files, symbols, edges),
                Style::default().fg(Color::Rgb(226, 232, 240)),
            ),
        ]),
    ];

    let info = Paragraph::new(lines).style(Style::default().bg(Color::Rgb(10, 10, 15)));
    f.render_widget(info, inner);
}

fn draw_footer(f: &mut Frame, area: Rect, progress: &Arc<Mutex<BuildProgress>>) {
    let prog = progress.lock().unwrap();
    let is_complete = prog.complete;
    drop(prog);

    let text = if is_complete {
        vec![
            Span::styled("[", Style::default().fg(Color::Rgb(71, 85, 105))),
            Span::styled("Enter/Space", Style::default().fg(Color::Rgb(52, 211, 153)).add_modifier(Modifier::BOLD)),
            Span::styled("] Continue  ", Style::default().fg(Color::Rgb(148, 163, 184))),
            Span::styled("[", Style::default().fg(Color::Rgb(71, 85, 105))),
            Span::styled("q", Style::default().fg(Color::Rgb(52, 211, 153)).add_modifier(Modifier::BOLD)),
            Span::styled("] Quit", Style::default().fg(Color::Rgb(148, 163, 184))),
        ]
    } else {
        vec![
            Span::styled("⚡ ", Style::default().fg(Color::Rgb(234, 179, 8))),
            Span::styled("Building code graph... ", Style::default().fg(Color::Rgb(100, 116, 139))),
            Span::styled("[", Style::default().fg(Color::Rgb(71, 85, 105))),
            Span::styled("q", Style::default().fg(Color::Rgb(52, 211, 153))),
            Span::styled("] Quit", Style::default().fg(Color::Rgb(100, 116, 139))),
        ]
    };

    let footer = Paragraph::new(Line::from(text))
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black));
    f.render_widget(footer, area);
}

// ============================================================================
// TUI Mode Implementations for All Commands
// ============================================================================

// Helper to draw simple header with subtitle


// TUI mode for overview
fn overview_tui_mode(graph: &CodeGraph) -> Result<()> {
    let stats = graph.stats();
    let files_result = graph_search(graph, "src/", 0);
    let mains = graph_search(graph, "main", 0);
    
    run_stateful_tui("Codebase Overview", move |f, area, ctx| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(10),    // Content
                Constraint::Length(1),  // Footer
            ])
            .split(area);

        // Header
        draw_animated_header(f, chunks[0], area.width, "Codebase Overview", ctx.frame);

        // Content
        let content_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::BORDER))
            .style(Style::default().bg(colors::BG_DARK))
            .title(Span::styled("  Overview  ", Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD)));

        let inner = content_block.inner(chunks[1]);
        f.render_widget(content_block, chunks[1]);
        
        // Helper to rebuild lines (in a real app we'd cache this, but it's cheap enough)
        // ... (lines construction code omitted, assuming we can reuse or just reconstruct)
        // Actually, to avoid rebuilding lines every frame, we should ideally compute them outside.
        // But for simplicity/safety with the borrow checker and existing structure, I'll reconstruct them.
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Files:   ", Style::default().fg(colors::TEXT_DIM)),
                Span::styled(format!("{}", stats.file_count), Style::default()
                    .fg(colors::ACCENT)
                    .add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Symbols: ", Style::default().fg(colors::TEXT_DIM)),
                Span::styled(format!("{}", stats.symbol_count), Style::default()
                    .fg(colors::ACCENT)
                    .add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Edges:   ", Style::default().fg(colors::TEXT_DIM)),
                Span::styled(format!("{}", stats.total_edges), Style::default()
                    .fg(colors::ACCENT)
                    .add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
        ];

        if !files_result.matched_files.is_empty() {
             lines.push(Line::from(Span::styled("Structure:", Style::default()
                .fg(colors::TEXT_BRIGHT)
                .add_modifier(Modifier::BOLD))));
            for file in files_result.matched_files.iter().take(50) { // increased limit
                 lines.push(Line::from(vec![
                    Span::raw("  📄 "),
                    Span::styled(format!("{}", file.display()), Style::default()
                        .fg(colors::TEXT_NORMAL)),
                ]));
            }
             if files_result.matched_files.len() > 50 {
                lines.push(Line::from(Span::styled(
                    format!("  ... and {} more", files_result.matched_files.len() - 50),
                    Style::default().fg(colors::TEXT_MUTED)
                )));
            }
            lines.push(Line::from(""));
        }

       if !mains.symbols.is_empty() {
            lines.push(Line::from(Span::styled("Entry Points:", Style::default()
                .fg(colors::TEXT_BRIGHT)
                .add_modifier(Modifier::BOLD))));
            for sym in mains.symbols.iter().filter(|s| s.name == "main") {
                lines.push(Line::from(vec![
                    Span::raw("  🚀 "),
                    Span::styled(&sym.name, Style::default().fg(colors::PRIMARY)),
                    Span::raw(" in "),
                    Span::styled(format!("{}", sym.file.display()), Style::default()
                        .fg(colors::TEXT_MUTED)),
                ]));
            }
        }

        // Set scroll state
        ctx.max_scroll = (lines.len() as u16).saturating_sub(inner.height);
        
        let content = Paragraph::new(lines)
            .style(Style::default().bg(colors::BG_DARK))
            .wrap(Wrap { trim: false })
            .scroll((ctx.scroll, 0));
            
        f.render_widget(content, inner);

        // Footer
        draw_scroll_footer(f, chunks[2], ctx);
        Ok(())
    })
}

// TUI mode for search
fn search_tui_mode(graph: &CodeGraph, query: &str, depth: usize) -> Result<()> {
    let result = graph_search(graph, query, depth);
    let query_str = query.to_string();
    
    run_stateful_tui(&format!("Search: '{}'", query), move |f, area, ctx| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(10),    // Content
                Constraint::Length(1),  // Footer
            ])
            .split(area);

        // Header
        draw_animated_header(f, chunks[0], area.width, &format!("Search: '{}'", query_str), ctx.frame);

        // Content
        let content_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::BORDER))
            .style(Style::default().bg(colors::BG_DARK))
            .title(Span::styled(format!("  Results (depth={})  ", depth), Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD)));

        let inner = content_block.inner(chunks[1]);
        f.render_widget(content_block, chunks[1]);

        let mut lines = Vec::new();

        if result.symbols.is_empty() && result.matched_files.is_empty() {
             lines.push(Line::from(Span::styled(
                "No results found",
                Style::default().fg(colors::TEXT_MUTED)
            )));
        } else {
             // ... (Optimized line generation similar to overview, recreating for brevity but logic stands)
             // For brevity in this replacement I'm copying the logic from the old function but increasing limits
             if !result.matched_files.is_empty() {
                 lines.push(Line::from(Span::styled(
                    format!("Files ({})", result.matched_files.len()),
                    Style::default().fg(colors::TEXT_BRIGHT).add_modifier(Modifier::BOLD)
                )));
                 for file in result.matched_files.iter() { // Display ALL files, allow scrolling
                    lines.push(Line::from(vec![
                        Span::raw("  📄 "),
                        Span::styled(format!("{}", file.display()), Style::default()
                            .fg(colors::ACCENT)),
                    ]));
                }
                lines.push(Line::from(""));
             }

             if !result.symbols.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("Symbols ({})", result.symbols.len()),
                    Style::default().fg(colors::TEXT_BRIGHT).add_modifier(Modifier::BOLD)
                )));
                for sym in result.symbols.iter() {
                     lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(&sym.name, Style::default()
                            .fg(colors::PRIMARY)
                            .add_modifier(Modifier::BOLD)),
                        Span::raw(" "),
                        Span::styled(format!("({})", sym.kind), Style::default()
                            .fg(colors::INFO)),
                        Span::raw(" - "),
                         Span::styled(format!("{}:{}", sym.file.display(), sym.line), Style::default()
                            .fg(colors::TEXT_MUTED)),
                    ]));
                }
                 lines.push(Line::from(""));
            }

             if !result.connections.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("Connections ({})", result.connections.len()),
                    Style::default().fg(colors::TEXT_BRIGHT).add_modifier(Modifier::BOLD)
                )));
                for conn in result.connections.iter() {
                     lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(&conn.from, Style::default().fg(colors::TEXT_NORMAL)),
                        Span::raw(" "),
                        Span::styled(format!("--[{}]->", conn.relationship), Style::default()
                            .fg(colors::WARNING)),
                        Span::raw(" "),
                        Span::styled(&conn.to, Style::default().fg(colors::TEXT_NORMAL)),
                    ]));
                }
            }
        }
        
        ctx.max_scroll = (lines.len() as u16).saturating_sub(inner.height);

        let content = Paragraph::new(lines)
            .style(Style::default().bg(colors::BG_DARK))
            .wrap(Wrap { trim: false })
            .scroll((ctx.scroll, 0));
        f.render_widget(content, inner);

        // Footer
        draw_scroll_footer(f, chunks[2], ctx);
        Ok(())
    })
}

// TUI mode for context
fn context_tui_mode(graph: &CodeGraph, query: &str, intent: &str) -> Result<()> {
    let result = get_context(graph, query, intent);
    let query_str = query.to_string();
    
    run_stateful_tui(&format!("Context: '{}'", query), move |f, area, ctx| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(10),    // Content
                Constraint::Length(1),  // Footer
            ])
            .split(area);

        // Header
        draw_animated_header(f, chunks[0], area.width, &format!("Context: '{}'", query_str), ctx.frame);

        // Content
        let content_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::BORDER))
            .style(Style::default().bg(colors::BG_DARK))
            .title(Span::styled("  Full Context  ", Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD)));

        let inner = content_block.inner(chunks[1]);
        f.render_widget(content_block, chunks[1]);

        // Display JSON result in a pretty format
        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        let lines: Vec<Line> = json.lines().map(|line| {
            Line::from(Span::styled(line, Style::default().fg(colors::TEXT_NORMAL)))
        }).collect();

        // Set scroll state
        ctx.max_scroll = (lines.len() as u16).saturating_sub(inner.height);

        let content = Paragraph::new(lines)
            .style(Style::default().bg(colors::BG_DARK))
            .wrap(Wrap { trim: false })
            .scroll((ctx.scroll, 0));
        f.render_widget(content, inner);

        // Footer
        draw_scroll_footer(f, chunks[2], ctx);
        Ok(())
    })
}

// TUI mode for deps
fn deps_tui_mode(graph: &CodeGraph, symbol: &str) -> Result<()> {
    let result = anchor_dependencies(graph, symbol);
    let symbol_str = symbol.to_string();
    
    run_stateful_tui(&format!("Dependencies: '{}'", symbol), move |f, area, ctx| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(10),    // Content
                Constraint::Length(1),  // Footer
            ])
            .split(area);

        // Header
        draw_animated_header(f, chunks[0], area.width, &format!("Deps: '{}'", symbol_str), ctx.frame);

        // Content
        let content_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::BORDER))
            .style(Style::default().bg(colors::BG_DARK))
            .title(Span::styled("  Dependency Graph  ", Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD)));

        let inner = content_block.inner(chunks[1]);
        f.render_widget(content_block, chunks[1]);

        // Display JSON result
        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        let lines: Vec<Line> = json.lines().map(|line| {
            Line::from(Span::styled(line, Style::default().fg(colors::TEXT_NORMAL)))
        }).collect();

        // Set scroll state
        ctx.max_scroll = (lines.len() as u16).saturating_sub(inner.height);

        let content = Paragraph::new(lines)
            .style(Style::default().bg(colors::BG_DARK))
            .wrap(Wrap { trim: false })
            .scroll((ctx.scroll, 0));
        f.render_widget(content, inner);

        // Footer
        draw_scroll_footer(f, chunks[2], ctx);
        Ok(())
    })
}

// TUI mode for stats
fn stats_tui_mode(graph: &CodeGraph) -> Result<()> {
    let stats = graph.stats();
    
    run_stateful_tui("Graph Statistics", move |f, area, ctx| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(10),    // Content
                Constraint::Length(1),  // Footer
            ])
            .split(area);

        // Header
        draw_animated_header(f, chunks[0], area.width, "Graph Statistics", ctx.frame);

        // Content
        let content_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::BORDER))
            .style(Style::default().bg(colors::BG_DARK))
            .title(Span::styled("  Statistics  ", Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD)));

        let inner = content_block.inner(chunks[1]);
        f.render_widget(content_block, chunks[1]);
        
        let max_val = stats.file_count.max(stats.symbol_count).max(stats.total_edges) as f64;
        let make_bar = |val: usize| -> String {
            let ratio = val as f64 / max_val;
            let bar_len = (ratio * 30.0) as usize;
            format!("{}{}", "█".repeat(bar_len), "░".repeat(30 - bar_len))
        };

        let lines = vec![
            Line::from(vec![
                Span::styled("Files:    ", Style::default().fg(colors::TEXT_DIM)),
                Span::styled(format!("{:>6} ", stats.file_count), Style::default()
                    .fg(colors::ACCENT)
                    .add_modifier(Modifier::BOLD)),
                Span::styled(make_bar(stats.file_count), Style::default().fg(colors::ACCENT)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Symbols:  ", Style::default().fg(colors::TEXT_DIM)),
                Span::styled(format!("{:>6} ", stats.symbol_count), Style::default()
                    .fg(colors::PRIMARY)
                    .add_modifier(Modifier::BOLD)),
                Span::styled(make_bar(stats.symbol_count), Style::default().fg(colors::PRIMARY)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Edges:    ", Style::default().fg(colors::TEXT_DIM)),
                Span::styled(format!("{:>6} ", stats.total_edges), Style::default()
                    .fg(colors::INFO)
                    .add_modifier(Modifier::BOLD)),
                Span::styled(make_bar(stats.total_edges), Style::default().fg(colors::INFO)),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("Graph Density: ", Style::default().fg(colors::TEXT_DIM)),
                Span::styled(
                    format!("{:.2}%", (stats.total_edges as f64 / stats.symbol_count.max(1) as f64) * 100.0),
                    Style::default().fg(colors::WARNING).add_modifier(Modifier::BOLD)
                ),
            ]),
        ];
        
        ctx.max_scroll = (lines.len() as u16).saturating_sub(inner.height);

        let content = Paragraph::new(lines)
            .style(Style::default().bg(colors::BG_DARK));
        f.render_widget(content, inner);

        // Footer
        draw_scroll_footer(f, chunks[2], ctx);
        Ok(())
    })
}

// Helper to run a stateful TUI
struct TUIContext {
    scroll: u16,
    max_scroll: u16,
    frame: usize,
    should_quit: bool,
    start_time: Instant,
}

impl TUIContext {
    fn new() -> Self {
        Self {
            scroll: 0,
            max_scroll: 0,
            frame: 0,
            should_quit: false,
            start_time: Instant::now(),
        }
    }

    fn on_tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }
    
    fn scroll_down(&mut self) {
        if self.scroll < self.max_scroll {
            self.scroll += 1;
        }
    }
    
    fn scroll_up(&mut self) {
        if self.scroll > 0 {
            self.scroll -= 1;
        }
    }
}

fn run_stateful_tui<F>(title: &str, mut draw_fn: F) -> Result<()>
where
    F: FnMut(&mut Frame, Rect, &mut TUIContext) -> Result<()>,
{
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut ctx = TUIContext::new();
    let tick_rate = Duration::from_millis(50);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| {
            let area = f.size();
            // Pass context mutably to draw_fn so it can update max_scroll
            let _ = draw_fn(f, area, &mut ctx);
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => ctx.should_quit = true,
                    KeyCode::Down | KeyCode::Char('j') => ctx.scroll_down(),
                    KeyCode::Up | KeyCode::Char('k') => ctx.scroll_up(),
                    KeyCode::PageDown => {
                        let amount = 10.min(ctx.max_scroll.saturating_sub(ctx.scroll));
                        ctx.scroll += amount;
                    }
                    KeyCode::PageUp => {
                        let amount = 10.min(ctx.scroll);
                        ctx.scroll -= amount;
                    }
                    KeyCode::Home => ctx.scroll = 0,
                    KeyCode::End => ctx.scroll = ctx.max_scroll,
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            ctx.on_tick();
            last_tick = Instant::now();
        }

        if ctx.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

// Updated draw helpers with animation support
fn draw_animated_header(f: &mut Frame, area: Rect, width: u16, subtitle: &str, frame: usize) {
    // Pulse effect for the logo color
    // Cycle roughly every 60 frames (~3 seconds at 20fps)
    let pulse = (frame as f64 / 10.0).sin().abs(); 
    let r = (52.0 + (16.0 - 52.0) * pulse) as u8;
    let g = (211.0 + (185.0 - 211.0) * pulse) as u8;
    let b = (153.0 + (129.0 - 153.0) * pulse) as u8;
    let brand_color = Color::Rgb(r, g, b);

    let logo = if width < 70 {
        vec![
            Line::from(vec![
                Span::styled(" ⚓ ", Style::default()
                    .fg(brand_color)
                    .add_modifier(Modifier::BOLD)),
                Span::styled("ANCHOR", Style::default()
                    .fg(brand_color)
                    .add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(subtitle, Style::default().fg(colors::TEXT_DIM)),
            ]),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled(" ⚓ ", Style::default()
                    .fg(brand_color)
                    .add_modifier(Modifier::BOLD)),
                Span::styled("ANCHOR", Style::default()
                    .fg(brand_color)
                    .add_modifier(Modifier::BOLD)),
                Span::styled("  •  ", Style::default().fg(colors::TEXT_FAINT)),
                Span::styled(subtitle, Style::default().fg(colors::TEXT_DIM)),
            ]),
        ]
    };

    let header = Paragraph::new(logo).style(Style::default().bg(Color::Black));
    f.render_widget(header, area);
}

fn draw_scroll_footer(f: &mut Frame, area: Rect, ctx: &TUIContext) {
    let scroll_percent = if ctx.max_scroll > 0 {
        format!("{:.0}%", (ctx.scroll as f64 / ctx.max_scroll as f64) * 100.0)
    } else {
        "Top".to_string()
    };

    let text = vec![
        Span::styled("[", Style::default().fg(colors::TEXT_FAINT)),
        Span::styled("↑↓/j/k", Style::default()
            .fg(colors::PRIMARY) // Use static primary for keys
            .add_modifier(Modifier::BOLD)),
        Span::styled("] Scroll  ", Style::default().fg(colors::TEXT_MUTED)),
        Span::styled(format!("({}) ", scroll_percent), Style::default().fg(colors::INFO)),
        Span::styled("[", Style::default().fg(colors::TEXT_FAINT)),
        Span::styled("q", Style::default()
            .fg(colors::PRIMARY)
            .add_modifier(Modifier::BOLD)),
        Span::styled("] Quit", Style::default().fg(colors::TEXT_MUTED)),
    ];

    let footer = Paragraph::new(Line::from(text))
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black));
    f.render_widget(footer, area);
}
