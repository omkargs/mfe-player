mod app;
mod artwork;
mod audio;
mod cava;
mod config;
mod daemon;
mod doctor;
mod ipc;
mod library;
mod models;
mod mpris;
mod plugins;
mod social;
mod state;
mod theme;
mod ui;
mod update;
mod wallpaper;

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
            if raw_args.iter().any(|arg| arg == "--json") {
                println!("{}", serde_json::to_string(&daemon::status_json())?);
            } else {
                println!("{}", daemon::status_line());
            }
            return Ok(());
        }
        Some("current") => {
            println!(
                "{}",
                daemon::status_json()["text"].as_str().unwrap_or("idle")
            );
            return Ok(());
        }
        Some("share") => {
            let state = state::PersistedState::load().0;
            match state.now_playing.as_ref() {
                Some(track) => {
                    let line = social::share_line(track);
                    println!("{line}");
                    match social::copy(&line) {
                        Ok(tool) => eprintln!("copied with {tool}"),
                        Err(err) => eprintln!("clipboard unavailable: {err}"),
                    }
                }
                None => println!("nothing playing"),
            }
            return Ok(());
        }
        Some("doctor") => {
            std::process::exit(doctor::run());
        }
        Some("update") => {
            let check_only = raw_args.iter().any(|arg| arg == "--check");
            update::run(check_only)?;
            return Ok(());
        }
        Some("cache") => {
            let artwork = config::artwork_dir();
            match raw_args.get(1).map(String::as_str) {
                Some("clean") | Some("clear") => {
                    let freed = dir_size(&artwork);
                    let _ = std::fs::remove_dir_all(&artwork);
                    println!("artwork cache cleared · freed {}", human_size(freed));
                }
                _ => {
                    println!(
                        "artwork: {} ({})",
                        artwork.display(),
                        human_size(dir_size(&artwork))
                    );
                    println!(
                        "downloads: {} ({})",
                        config::downloads_dir().display(),
                        human_size(dir_size(&config::downloads_dir()))
                    );
                    println!("run `mfe cache clean` to clear the artwork cache");
                }
            }
            return Ok(());
        }
        Some("theme") => {
            let (config, _) = config::load_config();
            match raw_args.get(1).map(String::as_str) {
                Some("adopt") | Some("wallpaper") => {
                    match theme::adopt_wallpaper(Some(&config.wallpaper)) {
                        Ok(path) => {
                            config::set_theme_in_file("wallpaper");
                            println!("theme: wallpaper · {path}");
                        }
                        Err(err) => {
                            eprintln!("could not adopt wallpaper: {err}");
                            std::process::exit(1);
                        }
                    }
                }
                Some("list") => {
                    for name in theme::THEME_NAMES {
                        println!("{name}");
                    }
                }
                _ => println!("{}", config.theme),
            }
            return Ok(());
        }
        Some("completions") => {
            let shell = raw_args.get(1).map(String::as_str).unwrap_or("bash");
            print!("{}", completions(shell));
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
    theme::apply_configured_with(&app_config.theme, Some(&app_config.wallpaper));
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

fn dir_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                dir_size(&path)
            } else {
                entry.metadata().map(|meta| meta.len()).unwrap_or(0)
            }
        })
        .sum()
}

fn human_size(bytes: u64) -> String {
    match bytes {
        0 => "empty".to_string(),
        bytes if bytes < 1024 => format!("{bytes} B"),
        bytes if bytes < 1024 * 1024 => format!("{:.1} KiB", bytes as f64 / 1024.0),
        bytes => format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0)),
    }
}

fn completions(shell: &str) -> String {
    match shell {
        "fish" => {
            r#"# mfe-player fish completions
complete -c mfe -f -a "status current share theme doctor cache update toggle stop completions" -d "MFE Player"
complete -c mfe -n "__fish_seen_subcommand_from status" -l json -d "JSON output"
complete -c mfe -n "__fish_seen_subcommand_from completions" -a "bash zsh fish"
"#
        }
        "zsh" => {
            r#"#compdef mfe mfe-player
_mfe() {
  local -a commands
  commands=(
    'status:show what is playing'
    'current:one-line now playing'
    'share:copy now playing to the clipboard'
    'theme:show or adopt a colour theme'
    'doctor:check mpv, plugins, config and clipboard'
    'cache:show or clear the artwork cache'
    'update:update to the latest release'
    'toggle:play/pause the background player'
    'stop:stop playback and the daemon'
    'completions:print shell completions'
  )
  _arguments -C \
    '1:command:->cmds' \
    '*::arg:->args'
  case $state in
    cmds) _describe 'command' commands ;;
    args)
      if [[ $words[2] == status ]]; then
        _arguments '--json[JSON output]'
      elif [[ $words[2] == completions ]]; then
        _values 'shell' bash zsh fish
      elif [[ $words[2] == theme ]]; then
        _values 'action' adopt list
      fi
      ;;
  esac
}
_mfe "$@"
"#
        }
        _ => {
            r#"# mfe-player bash completions
_mfe_player() {
  local cur prev
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"
  if [ "$COMP_CWORD" -eq 1 ]; then
    COMPREPLY=( $(compgen -W "status current share theme doctor cache update toggle stop completions" -- "$cur") )
  elif [ "$prev" = "completions" ]; then
    COMPREPLY=( $(compgen -W "bash zsh fish" -- "$cur") )
  elif [ "$prev" = "theme" ]; then
    COMPREPLY=( $(compgen -W "adopt list" -- "$cur") )
  elif [ "$prev" = "cache" ]; then
    COMPREPLY=( $(compgen -W "clean" -- "$cur") )
  elif [ "$prev" = "update" ]; then
    COMPREPLY=( $(compgen -W "--check" -- "$cur") )
  elif [ "$prev" = "status" ]; then
    COMPREPLY=( $(compgen -W "--json" -- "$cur") )
  fi
}
complete -F _mfe_player mfe mfe-player
"#
        }
    }
    .to_string()
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
               mfe-player status --json   waybar/polybar/tmux friendly JSON
               mfe-player theme           print the current theme
               mfe-player theme list      every built-in theme
               mfe-player theme adopt     derive a theme from your wallpaper
               mfe-player doctor          check mpv, plugins, config, clipboard
               mfe-player update          update to the latest release (--check)
               mfe-player cache [clean]   show or clear the artwork cache\n\
               mfe-player current         one-line now playing\n\
               mfe-player share           copy now playing to the clipboard\n\
               mfe-player toggle          play/pause the background player\n\
               mfe-player stop            stop playback and the daemon\n\
               mfe-player completions bash|zsh|fish\n\n\
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
