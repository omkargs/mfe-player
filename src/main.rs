mod app;
mod audio;
mod cava;
mod config;
mod daemon;
mod models;
mod mpris;
mod plugins;
mod state;
mod theme;
mod ui;

use std::io::{self, Stdout};
use std::panic;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

fn main() -> Result<()> {
    config::migrate_legacy();
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    if raw_args.iter().any(|arg| arg == "--daemon") {
        return daemon::run();
    }
    match raw_args.first().map(String::as_str) {
        Some("stop") => {
            daemon::stop_everything()?;
            println!("mfe-player: stopped");
            return Ok(());
        }
        Some("toggle") => {
            daemon::toggle_pause()?;
            return Ok(());
        }
        Some("status") => {
            println!("{}", daemon::status_line());
            return Ok(());
        }
        _ => {}
    }
    let initial_query = match cli_action() {
        CliAction::Exit => return Ok(()),
        CliAction::Run(query) => query,
    };

    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));

    let (app_config, config_warning) = config::load_config();
    let mouse = app_config.mouse;
    theme::apply_configured(&app_config.theme);
    let mut app = app::App::new(app_config)?;
    if let Some(warning) = config_warning {
        app.flash(warning);
    }
    if let Some(query) = initial_query {
        app.prime_search(query);
    }
    let mut terminal = setup_terminal(mouse)?;
    let result = run(&mut terminal, &mut app);
    restore_terminal(&mut terminal, mouse)?;
    app.persist();

    if let Err(err) = result {
        eprintln!("mfe-player: {err:#}");
    }

    if app.detached_on_quit {
        println!();
        println!("{}", app.background_hint());
        println!();
    }
    Ok(())
}

enum CliAction {
    Run(Option<String>),
    Exit,
}

fn cli_action() -> CliAction {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("mfe-player {}", env!("CARGO_PKG_VERSION"));
        return CliAction::Exit;
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "mfe-player {}\n\
             A plugin-driven music streaming player for the terminal.\n\n\
             Usage: mfe-player [query]\n\
               mfe-player                 open the TUI\n\
               mfe-player daft punk       open the TUI with search results\n\
               mfe-player status          what is playing right now?\n\
               mfe-player toggle          play/pause the background player\n\
               mfe-player stop            stop playback and the daemon\n\n\
             First run installs default plugins into ~/.config/mfe-player/plugins.\n\
             Config: ~/.config/mfe-player/config.toml (themes, steps, cava, ...).\n\
             Press ? inside the app for the full keymap.\n\n\
             Essential keys:\n\
               /        focus search (Ctrl+F filters the current list)\n\
               Enter    play selected / open artist, album or playlist\n\
               Space    play or pause (also starts a restored queue)\n\
               h        go back (artist/album page, open playlist)\n\
               Shift+←/→ seek −30s / +30s\n\
               y        lyrics\n\
               Ctrl+P   fzf-style command palette\n\
               Ctrl+B   quit the TUI, keep playing (repeat + queue keep running)\n\
               q        quit and stop audio",
            env!("CARGO_PKG_VERSION")
        );
        return CliAction::Exit;
    }
    let query = args
        .iter()
        .filter(|arg| !arg.starts_with('-'))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    CliAction::Run((!query.trim().is_empty()).then_some(query))
}

fn setup_terminal(mouse: bool) -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    if mouse {
        execute!(stdout, EnableMouseCapture)?;
    }
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>, mouse: bool) -> Result<()> {
    disable_raw_mode()?;
    if mouse {
        execute!(terminal.backend_mut(), DisableMouseCapture)?;
    }
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut app::App) -> Result<()> {
    while !app.should_quit {
        app.tick();
        terminal.draw(|frame| ui::draw(frame, app))?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    app.handle_key(key);
                }
                Event::Mouse(mouse) => app.handle_mouse(mouse),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
    Ok(())
}
