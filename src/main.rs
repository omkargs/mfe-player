mod app;
mod artwork;
mod audio;
mod cava;
mod config;
mod daemon;
mod discord;
mod doctor;
mod ipc;
mod library;
mod models;
#[cfg(target_os = "linux")]
mod mpris;
mod notify;
mod platform;
mod plugins;
mod social;
mod state;
mod taste;
mod theme;
mod ui;
mod update;
mod wallpaper;

use std::io::{self, Stdout};
use std::panic;
use std::time::Duration;

use anyhow::Result;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};

/// Set from a signal handler when the window/terminal is closed externally.
/// The TUI must not die here: mpv and MPRIS would disappear while the detached
/// audio keeps playing. Instead the loop hands both to the background daemon.
#[cfg(unix)]
static EXTERNAL_CLOSE: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_external_close(_: libc::c_int) {
    EXTERNAL_CLOSE.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
fn install_external_close_handlers() {
    unsafe {
        for signal in [libc::SIGHUP, libc::SIGTERM, libc::SIGINT] {
            libc::signal(signal, on_external_close as *const () as libc::sighandler_t);
        }
    }
}

#[cfg(not(unix))]
fn install_external_close_handlers() {}

fn external_close_requested() -> bool {
    #[cfg(unix)]
    {
        EXTERNAL_CLOSE.load(Ordering::SeqCst)
    }
    #[cfg(not(unix))]
    {
        false
    }
}
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

fn main() -> Result<()> {
    // CLI output is routinely piped (`mfe taste | head`). The default Rust
    // behaviour for a closed pipe is a panic; a Unix tool should exit quietly.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
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
                    println!(
                        "limit: {} (least-recently-used covers go first · MFE_ARTWORK_CACHE_MB overrides)",
                        human_size(artwork::cache_limit())
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
        Some("discord") => {
            let (app_config, _) = config::load_config();
            let action = raw_args.get(1).map(String::as_str).unwrap_or("status");
            match action {
                "on" | "enable" => {
                    config::set_line("discord", "true");
                    println!("discord presence enabled");
                    println!(
                        "  shares: track title, artist, album and playback progress over Discord's local IPC socket"
                    );
                    println!("  never shares: file paths, listening history, tokens or IPs");
                    if app_config.discord_client_id.trim().is_empty()
                        && std::env::var("MFE_DISCORD_CLIENT_ID").is_err()
                    {
                        println!();
                        println!("next: set an application client id with");
                        println!("  mfe discord client <id>   (or export MFE_DISCORD_CLIENT_ID)");
                        println!("create one at https://discord.com/developers/applications");
                    }
                    println!("restart mfe-player (or the daemon) to apply");
                }
                "off" | "disable" => {
                    config::set_line("discord", "false");
                    println!("discord presence disabled");
                    println!("restart mfe-player (or the daemon) to apply");
                }
                "client" | "id" => match raw_args.get(2) {
                    Some(id) if !id.trim().is_empty() => {
                        config::set_line("discord_client_id", &format!("\"{}\"", id.trim()));
                        config::set_line("discord", "true");
                        println!("discord client id set · presence enabled");
                    }
                    _ => eprintln!("usage: mfe discord client <application-id>"),
                },
                "asset" => match raw_args.get(2) {
                    Some(asset) if !asset.trim().is_empty() => {
                        config::set_line("discord_asset", &format!("\"{}\"", asset.trim()));
                        println!("discord large-image asset set to “{}”", asset.trim());
                    }
                    _ => eprintln!("usage: mfe discord asset <key>"),
                },
                "status" => {
                    let id = if !app_config.discord_client_id.trim().is_empty() {
                        app_config.discord_client_id.trim().to_string()
                    } else {
                        std::env::var("MFE_DISCORD_CLIENT_ID").unwrap_or_default()
                    };
                    println!("discord presence");
                    println!(
                        "  enabled   {}",
                        if app_config.discord { "yes" } else { "no" }
                    );
                    println!(
                        "  client id {}",
                        if id.is_empty() {
                            "not set"
                        } else {
                            id.as_str()
                        }
                    );
                    println!("  asset     {}", app_config.discord_asset);
                    println!(
                        "  paused    {}",
                        if app_config.discord_show_paused {
                            "shown"
                        } else {
                            "hidden"
                        }
                    );
                    println!(
                        "  buttons   {}",
                        if app_config.discord_show_buttons {
                            "on"
                        } else {
                            "off"
                        }
                    );
                    println!(
                        "  discord   {}",
                        if discord::socket_present() {
                            "desktop client detected"
                        } else {
                            "not running (presence is inert)"
                        }
                    );
                    if app_config.discord && id.is_empty() {
                        println!();
                        println!("presence stays off until a client id is configured.");
                        println!("run: mfe discord client <id>");
                    }
                }
                other => {
                    eprintln!("usage: mfe discord [on|off|status|client <id>|asset <key>]");
                    eprintln!("unknown action: {other}");
                }
            }
            return Ok(());
        }
        Some("motion") => {
            let (app_config, _) = config::load_config();
            match raw_args.get(1).map(String::as_str).unwrap_or("status") {
                "on" | "enable" => {
                    config::set_line("motion", "true");
                    println!("motion on · gradients, marquees and borders follow cava");
                }
                "off" | "disable" => {
                    config::set_line("motion", "false");
                    println!("motion off · static frames (relaunch the TUI to apply)");
                }
                "status" => {
                    println!("interface motion");
                    println!(
                        "  motion    {}",
                        if app_config.motion { "on" } else { "off" }
                    );
                    println!("  source    cava energy (bass / mid / treble)");
                    println!("  toggle    Alt+M in the TUI");
                }
                other => {
                    eprintln!("usage: mfe motion [on|off|status]");
                    eprintln!("unknown action: {other}");
                }
            }
            return Ok(());
        }
        Some("taste") => {
            let (mut taste, warning) = taste::Taste::load();
            if let Some(warning) = warning {
                eprintln!("mfe-player: {warning}");
            }
            match raw_args.get(1).map(String::as_str).unwrap_or("status") {
                "status" | "summary" => {
                    print!("{}", taste.summary());
                }
                "explain" | "last" => {
                    print!("{}", taste.explain_last());
                }
                "reset" => {
                    let scope = raw_args.get(2).map(String::as_str).unwrap_or("all");
                    if !["session", "all", "probes"].contains(&scope) {
                        eprintln!("usage: mfe taste reset [session|all|probes]");
                        return Ok(());
                    }
                    taste.reset(scope);
                    taste.flush();
                    println!("taste reset ({scope})");
                }
                "like" | "dislike" => {
                    let state = state::PersistedState::load().0;
                    match state.now_playing.as_ref() {
                        Some(track) => {
                            if raw_args[1] == "like" {
                                taste.like(track);
                                taste.flush();
                                println!("liked “{}” — {}", track.title, track.artist);
                            } else {
                                taste.dislike(track);
                                taste.flush();
                                println!(
                                    "disliked “{}” — {} (blocked for 30 days)",
                                    track.title, track.artist
                                );
                            }
                        }
                        None => println!("nothing is playing"),
                    }
                }
                _ => {
                    eprintln!(
                        "usage: mfe taste [status|explain|reset [session|all|probes]|like|dislike]"
                    );
                }
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
complete -c mfe -f -a "status current share theme taste discord motion doctor cache update toggle stop completions" -d "MFE Player"
complete -c mfe -n "__fish_seen_subcommand_from status" -l json -d "JSON output"
complete -c mfe -n "__fish_seen_subcommand_from completions" -a "bash zsh fish powershell"
complete -c mfe -n "__fish_seen_subcommand_from taste" -a "status explain reset like dislike" -d "Taste profile"
complete -c mfe -n "__fish_seen_subcommand_from discord" -a "on off status client asset" -d "Discord presence"
complete -c mfe -n "__fish_seen_subcommand_from motion" -a "on off status" -d "Interface motion"
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
    'taste:show or reset what autoplay has learned'
    'discord:configure Discord Rich Presence'
    'motion:toggle reactive interface motion'
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
        _values 'shell' bash zsh fish powershell
      elif [[ $words[2] == theme ]]; then
        _values 'action' adopt list
      elif [[ $words[2] == taste ]]; then
        _values 'action' status explain reset like dislike
      elif [[ $words[2] == discord ]]; then
        _values 'action' on off status client asset
      elif [[ $words[2] == motion ]]; then
        _values 'action' on off status
      fi
      ;;
  esac
}
_mfe "$@"
"#
        }
        "powershell" | "pwsh" => {
            r#"# mfe-player PowerShell completions
Register-ArgumentCompleter -Native -CommandName mfe,mfe-player -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $commands = @('status','current','share','theme','taste','discord','motion','doctor','cache','update','toggle','stop','completions')
    $words = $commandAst.CommandElements | ForEach-Object { $_.ToString() }
    if ($words.Count -le 2) {
        $commands | Where-Object { $_ -like "$wordToComplete*" } |
            ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }
    } elseif ($words[1] -eq 'completions') {
        @('bash','zsh','fish','powershell') | Where-Object { $_ -like "$wordToComplete*" } |
            ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }
    } elseif ($words[1] -eq 'discord') {
        @('on','off','status','client','asset') | Where-Object { $_ -like "$wordToComplete*" } |
            ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }
    } elseif ($words[1] -eq 'motion') {
        @('on','off','status') | Where-Object { $_ -like "$wordToComplete*" } |
            ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }
    } elseif ($words[1] -eq 'taste') {
        @('status','explain','reset','like','dislike') | Where-Object { $_ -like "$wordToComplete*" } |
            ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }
    }
}
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
    COMPREPLY=( $(compgen -W "status current share theme taste discord motion doctor cache update toggle stop completions" -- "$cur") )
  elif [ "$prev" = "completions" ]; then
    COMPREPLY=( $(compgen -W "bash zsh fish powershell" -- "$cur") )
  elif [ "$prev" = "theme" ]; then
    COMPREPLY=( $(compgen -W "adopt list" -- "$cur") )
  elif [ "$prev" = "taste" ]; then
    COMPREPLY=( $(compgen -W "status explain reset like dislike" -- "$cur") )
  elif [ "$prev" = "discord" ]; then
    COMPREPLY=( $(compgen -W "on off status client asset" -- "$cur") )
  elif [ "$prev" = "motion" ]; then
    COMPREPLY=( $(compgen -W "on off status" -- "$cur") )
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
               mfe-player taste           what autoplay has learned\n\
               mfe-player taste explain   why the last autoplay pick was chosen\n\
               mfe-player taste reset     forget the profile (session|all|probes)\n\
               mfe-player discord         Discord Rich Presence status\n\
               mfe-player discord on|off  enable or disable presence\n\
               mfe-player discord client <id>   set the Discord application id\n\
               mfe-player motion          animation status (on|off)\n\
               mfe-player toggle          play/pause the background player\n\
               mfe-player stop            stop playback and the daemon\n\
               mfe-player completions bash|zsh|fish|powershell\n\n\
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
    install_external_close_handlers();
    let mut title = String::new();
    while !app.should_quit {
        app.tick();
        if external_close_requested() {
            app.handoff_for_external_close();
            break;
        }
        if let Err(err) = terminal.draw(|frame| ui::draw(frame, app)) {
            // A closed terminal makes every later write fail. Keep the music
            // (and its MPRIS card) alive by handing it to the daemon instead
            // of dying with the window.
            app.handoff_for_external_close();
            if !app.detached_on_quit {
                return Err(err.into());
            }
            break;
        }

        let wanted = app.terminal_title();
        if wanted != title {
            if execute!(terminal.backend_mut(), SetTitle(&wanted)).is_err() {
                app.handoff_for_external_close();
                break;
            }
            title = wanted;
        }

        if external_close_requested() {
            app.handoff_for_external_close();
            break;
        }

        // `poll`/`read` can fail with EINTR when a signal arrives mid-wait.
        // That is not an error: the flag above is the real message.
        match event::poll(Duration::from_millis(50)) {
            Ok(false) => {}
            Ok(true) => match event::read() {
                Ok(Event::Key(key))
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    app.handle_key(key);
                }
                Ok(Event::Mouse(mouse)) => app.handle_mouse(mouse),
                Ok(Event::Resize(_, _)) => {}
                Ok(_) => {}
                Err(err) => {
                    if external_close_requested() {
                        app.handoff_for_external_close();
                        break;
                    }
                    return Err(err.into());
                }
            },
            Err(err) => {
                if external_close_requested() {
                    app.handoff_for_external_close();
                    break;
                }
                return Err(err.into());
            }
        }
    }
    let _ = execute!(terminal.backend_mut(), SetTitle("mfe-player"));
    Ok(())
}
