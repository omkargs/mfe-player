use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Color;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{
    palette_ranked, App, FilterKind, HitAction, Overlay, PaletteItem, View, PALETTE_ITEMS,
    SLEEP_OPTIONS,
};
use crate::models::{display_width, format_duration, format_position, truncate, Track};
use crate::plugins::PluginKind;
use crate::theme;

fn th() -> theme::Theme {
    theme::get()
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.hits.clear();
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::bg()).fg(th().fg)),
        area,
    );

    // Below this there is no honest way to show lists, borders and a player
    // bar at once. Say so instead of drawing a shredded layout.
    if area.width < 24 || area.height < 10 {
        frame.render_widget(
            Paragraph::new(format!(
                "mfe · {}×{} — terminal too small",
                area.width, area.height
            ))
            .alignment(Alignment::Center)
            .style(Style::default().fg(th().dim).bg(theme::bg())),
            area,
        );
        if app.ascii_icons() {
            ascii_safe(frame);
        }
        return;
    }

    // Squeeze the chrome on short terminals before squeezing the list.
    let header = if area.height >= 16 { 3 } else { 1 };
    let player = if area.height >= 14 { 4 } else { 3 };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header),
            Constraint::Min(5),
            Constraint::Length(player),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, app, rows[0]);
    draw_body(frame, app, rows[1]);
    draw_player(frame, app, rows[2]);
    draw_footer(frame, app, rows[3]);

    if app.overlay.is_some() {
        draw_overlay(frame, app, area);
    }

    if app.ascii_icons() {
        ascii_safe(frame);
    }
}

/// Replace decorative glyphs in the rendered buffer with plain ASCII. Text is
/// left alone, so titles, artists and lyrics keep their characters.
fn ascii_safe(frame: &mut Frame) {
    for cell in frame.buffer_mut().content.iter_mut() {
        if let Some(replacement) = ascii_glyph(cell.symbol()) {
            cell.set_symbol(replacement);
        }
    }
}

fn ascii_glyph(symbol: &str) -> Option<&'static str> {
    let first = symbol.chars().next()?;
    if ('\u{2800}'..='\u{28ff}').contains(&first) {
        return Some("#");
    }
    Some(match symbol {
        "╭" | "╮" | "╰" | "╯" | "┌" | "┐" | "└" | "┘" | "╔" | "╗" | "╚" | "╝" | "├" | "┤" | "┬"
        | "┴" | "┼" => "+",
        "─" | "━" | "╌" | "╍" | "═" => "-",
        "│" | "┃" | "╎" | "╏" | "║" => "|",
        "▶" | "▸" | "►" => ">",
        "◀" | "◂" | "◄" => "<",
        "▲" => "^",
        "▼" => "v",
        "●" | "•" | "◉" | "⬤" => "*",
        "○" | "◌" => "o",
        "✓" | "✔" => "x",
        "✗" | "✘" | "×" => "x",
        "★" | "☆" | "✦" | "✧" => "*",
        "≡" => "=",
        "⌕" => "?",
        "⤓" | "⬇" => "v",
        "⚙" => "*",
        "◷" | "⏱" => "o",
        "♫" | "♪" | "♬" => "~",
        "▤" => "=",
        "▰" => "#",
        "▱" | "▁" => "-",
        "▏" => "'",
        "▎" | "▍" | "▌" | "▋" | "▊" | "▉" | "█" | "▂" | "▃" | "▅" | "▆" | "▇" | "░" | "▒" | "▓" => {
            "="
        }
        "▀" | "▄" => "#",
        "⏸" => "||",
        "⏭" => ">>",
        "⏮" => "<<",
        "…" => "...",
        "·" => ".",
        "—" | "–" => "-",
        _ => return None,
    })
}

/// Unit-testable mapping used by [`ascii_safe`].
#[cfg(test)]
mod ascii_tests {
    use super::ascii_glyph;

    #[test]
    fn maps_decoration_but_not_text() {
        assert_eq!(ascii_glyph("▶"), Some(">"));
        assert_eq!(ascii_glyph("╭"), Some("+"));
        assert_eq!(ascii_glyph("█"), Some("="));
        assert_eq!(ascii_glyph("★"), Some("*"));
        assert_eq!(ascii_glyph("A"), None);
        assert_eq!(ascii_glyph("é"), None);
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let top =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).split(area);

    let mut logo_spans = vec![Span::styled(
        " ▰ ",
        Style::default()
            .fg(th().accent)
            .add_modifier(Modifier::BOLD),
    )];
    logo_spans.extend(gradient_text(
        "MFE PLAYER",
        th().accent,
        th().accent2,
        Modifier::BOLD,
    ));
    logo_spans.push(Span::styled(
        "   music for everyone",
        Style::default().fg(th().dim).add_modifier(Modifier::ITALIC),
    ));
    let logo = Line::from(logo_spans);
    frame.render_widget(
        Paragraph::new(logo).style(Style::default().bg(theme::bg())),
        top[0],
    );

    let status = if let Some((toast, _)) = &app.toast {
        toast.as_str()
    } else {
        app.status.as_str()
    };
    let status_color = if status.contains("error") || status.contains("failed") {
        th().red
    } else if app.resolving {
        th().yellow
    } else {
        th().dim
    };
    let status_line = Line::from(vec![
        Span::styled(
            truncate(status, top[1].width.saturating_sub(2) as usize),
            Style::default().fg(status_color),
        ),
        Span::raw(" "),
    ])
    .alignment(Alignment::Right);
    frame.render_widget(
        Paragraph::new(status_line).style(Style::default().bg(theme::bg())),
        top[1],
    );

    let meta = app.active_plugin_name(PluginKind::Metadata);
    let streaming = app.active_plugin_name(PluginKind::Streaming);
    let lyrics = app.active_plugin_name(PluginKind::Lyrics);
    let line2 = Line::from(vec![
        Span::styled(" meta ", Style::default().fg(th().faint)),
        Span::styled(meta, Style::default().fg(th().accent2)),
        Span::styled("  ·  stream ", Style::default().fg(th().faint)),
        Span::styled(streaming, Style::default().fg(th().green)),
        Span::styled("  ·  lyrics ", Style::default().fg(th().faint)),
        Span::styled(lyrics, Style::default().fg(th().purple)),
        Span::styled("  ·  ", Style::default().fg(th().faint)),
        Span::styled(
            format!("{} plugins", app.plugins.installed.len()),
            Style::default().fg(th().dim),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line2).style(Style::default().bg(theme::bg())),
        Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: 1,
        },
    );

    if area.height < 3 {
        return;
    }

    let rule = "─".repeat(area.width as usize);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            rule,
            Style::default().fg(th().border),
        ))),
        Rect {
            x: area.x,
            y: area.y + 2,
            width: area.width,
            height: 1,
        },
    );
}

fn draw_body(frame: &mut Frame, app: &mut App, area: Rect) {
    let cava_right = app.cava_enabled
        && app.cava.is_some()
        && app.config.cava_position.eq_ignore_ascii_case("right");
    let cava_width = if cava_right && area.width >= 96 {
        26
    } else {
        0
    };
    let detail_width = if area.width >= 112 && cava_width == 0 {
        34
    } else {
        0
    };
    let cava_left = app.cava_enabled && cava_width == 0;

    // Narrow terminals get the list, not a 22-column sidebar glued to a
    // 16-column list. The number keys still switch views.
    let sidebar = area.width >= 76;
    let mut constraints = Vec::new();
    if sidebar {
        constraints.push(Constraint::Length(22));
    }
    constraints.push(Constraint::Min(20));
    if detail_width > 0 {
        constraints.push(Constraint::Length(detail_width));
    }
    if cava_width > 0 {
        constraints.push(Constraint::Length(cava_width));
    }
    let cols = Layout::horizontal(constraints).split(area);

    let mut next = 0;
    if sidebar {
        draw_sidebar(frame, app, cols[next], cava_left);
        next += 1;
    }
    draw_main(frame, app, cols[next]);
    next += 1;
    if detail_width > 0 {
        draw_inspector(frame, app, cols[next]);
        next += 1;
    }
    if cava_width > 0 {
        draw_cava_panel(frame, app, cols[next]);
    }
}

fn draw_sidebar(frame: &mut Frame, app: &mut App, area: Rect, cava_left: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().border))
        .title(Span::styled(" library ", Style::default().fg(th().dim)))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = if cava_left {
        Layout::vertical([
            Constraint::Min(6),
            Constraint::Length(9),
            Constraint::Length(8),
        ])
        .split(inner)
    } else {
        Layout::vertical([Constraint::Min(8), Constraint::Length(8)]).split(inner)
    };

    let mut lines = Vec::new();
    for (index, view) in View::ALL.iter().enumerate() {
        let selected = app.view == *view;
        let count = match view {
            View::Search => app.search_results.len(),
            View::Queue => app.queue.len(),
            View::Favorites => app.favorites.len(),
            View::Playlists => app.playlists.len(),
            View::Downloads => app.downloads.len(),
            View::Plugins => app.plugin_entries().len(),
            View::History => app.history.len(),
            View::Library => app.library.len(),
            View::Browse => app
                .browse_page()
                .map(|page| page.albums.len() + page.tracks.len())
                .unwrap_or(0),
        };
        let style = if selected {
            Style::default()
                .bg(th().accent)
                .fg(th().bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(th().fg)
        };
        let label = format!(
            " {} {} {:<12}{:>3}",
            index + 1,
            view.icon(),
            view.title(),
            count
        );
        let label = truncate(&label, rows[0].width as usize);
        lines.push(Line::from(Span::styled(label, style)));
    }
    lines.push(Line::from(""));
    let playing = app
        .now_playing
        .as_ref()
        .map(|track| format!(" ▶ {}", truncate(&track.title, 14)))
        .unwrap_or_else(|| " ▶ nothing playing".to_string());
    lines.push(Line::from(Span::styled(
        playing,
        Style::default().fg(if app.now_playing.is_some() {
            th().green
        } else {
            th().faint
        }),
    )));
    lines.push(Line::from(Span::styled(
        format!(" {} queued", app.queue.len()),
        Style::default().fg(th().dim),
    )));
    lines.push(Line::from(Span::styled(
        format!(" {:.0}% vol", app.audio_snapshot().volume.max(app.volume)),
        Style::default().fg(th().dim),
    )));
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme::panel()))
            .wrap(Wrap { trim: true }),
        rows[0],
    );
    let view_hits: Vec<(usize, HitAction)> = View::ALL
        .iter()
        .enumerate()
        .map(|(index, view)| (index, HitAction::SwitchView(*view)))
        .collect();
    app.record_hits(rows[0], 0, view_hits);

    let low = vec![
        Line::from(Span::styled(" controls", Style::default().fg(th().faint))),
        Line::from(vec![
            Span::styled(" /", Style::default().fg(th().accent2)),
            Span::styled(" search/filter", Style::default().fg(th().dim)),
        ]),
        Line::from(vec![
            Span::styled(" ?", Style::default().fg(th().accent2)),
            Span::styled(" help", Style::default().fg(th().dim)),
            Span::styled("   Ctrl+P", Style::default().fg(th().accent2)),
            Span::styled(" cmd", Style::default().fg(th().dim)),
        ]),
        Line::from(vec![
            Span::styled(" Ctrl+B", Style::default().fg(th().accent2)),
            Span::styled(" background", Style::default().fg(th().dim)),
        ]),
        Line::from(vec![
            Span::styled(" q", Style::default().fg(th().accent2)),
            Span::styled(" quit", Style::default().fg(th().dim)),
        ]),
    ];
    let controls_row = if cava_left { rows[2] } else { rows[1] };
    frame.render_widget(
        Paragraph::new(low).style(Style::default().bg(theme::panel())),
        controls_row,
    );
    if cava_left {
        draw_cava_panel(frame, app, rows[1]);
    }
}

fn draw_cava_panel(frame: &mut Frame, app: &App, area: Rect) {
    if area.height < 3 || area.width < 6 {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().border))
        .title(Span::styled(
            " ♪ spectrum ",
            Style::default().fg(th().accent2),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let Some(cava) = app.cava.as_ref() else {
        frame.render_widget(
            Paragraph::new(vec![Line::from(Span::styled(
                " visualiser off ",
                Style::default().fg(th().faint),
            ))])
            .style(Style::default().bg(theme::panel())),
            inner,
        );
        return;
    };
    if let Some(err) = cava.error() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    " cava unavailable ",
                    Style::default().fg(th().yellow),
                )),
                Line::from(Span::styled(
                    format!(" {err}"),
                    Style::default().fg(th().dim),
                )),
                Line::from(Span::styled(
                    " Ctrl+V toggles ",
                    Style::default().fg(th().faint),
                )),
            ])
            .style(Style::default().bg(theme::panel()))
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }

    if !cava.running() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    " cava is not running ",
                    Style::default().fg(th().yellow),
                )),
                Line::from(Span::styled(
                    " Ctrl+V restarts it",
                    Style::default().fg(th().faint),
                )),
            ])
            .style(Style::default().bg(theme::panel())),
            inner,
        );
        return;
    }

    let values = cava.values();
    if values.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " waiting for audio… ",
                Style::default().fg(th().faint),
            ))
            .style(Style::default().bg(theme::panel())),
            inner,
        );
        return;
    }

    let width = inner.width as usize;
    let height = inner.height as usize;
    let mut lines = Vec::new();
    for row in 0..height {
        let mut spans = Vec::new();
        for col in 0..width {
            let index = col * values.len() / width.max(1);
            let value = values.get(index).copied().unwrap_or(0.0);
            let level = value * height as f64;
            let threshold = (height - row) as f64;
            let (glyph, color) = if level >= threshold {
                (
                    "█",
                    if value > 0.72 {
                        th().accent
                    } else if value > 0.38 {
                        th().accent2
                    } else {
                        th().green
                    },
                )
            } else if level > threshold - 0.85 {
                ("▄", th().accent2)
            } else {
                (" ", th().faint)
            };
            spans.push(Span::styled(glyph, Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel())),
        inner,
    );
}

fn draw_main(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = match app.view {
        View::Search => format!(
            " ⌕ search · {} {} ",
            app.active_plugin_name(PluginKind::Metadata),
            if app.search_loading {
                app.spinner_char()
            } else {
                ""
            }
        ),
        View::Queue => format!(
            " ≡ queue · {} tracks · {} ",
            app.queue.len(),
            format_duration(app.current_queue_duration())
        ),
        View::Favorites => format!(" ★ favorites · {} tracks ", app.favorites.len()),
        View::Playlists => {
            if let Some(index) = app.open_playlist {
                if let Some(playlist) = app.playlists.get(index) {
                    format!(
                        " ▤ playlists / {} · {} tracks ",
                        playlist.name,
                        playlist.tracks.len()
                    )
                } else {
                    " ▤ playlists ".to_string()
                }
            } else {
                format!(" ▤ playlists · {} ", app.playlists.len())
            }
        }
        View::Downloads => format!(
            " ⤓ downloads · {} offline tracks · d download · x delete ",
            app.downloads.len()
        ),
        View::Plugins => " ⚙ plugin store · Enter activate · i install · u uninstall ".to_string(),
        View::History => format!(
            " ◷ history · {} plays · Enter replay · Ctrl+G stats ",
            app.history.len()
        ),
        View::Library => format!(
            " ♫ local library · {} tracks{} · R rescan ",
            app.library.len(),
            if app.library_scanning {
                " · scanning…"
            } else {
                ""
            }
        ),
        View::Browse => app
            .browse_page()
            .map(|page| format!(" {} · {} ", page.title, page.subtitle))
            .unwrap_or_else(|| " ◉ browse ".to_string()),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().border))
        .title(Span::styled(title, Style::default().fg(th().dim)))
        .style(Style::default().bg(theme::panel2()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match app.view {
        View::Search => draw_search(frame, app, inner),
        View::Queue => draw_queue(frame, app, inner),
        View::Favorites => draw_favorites(frame, app, inner),
        View::Playlists => draw_playlists(frame, app, inner),
        View::Downloads => draw_downloads(frame, app, inner),
        View::Plugins => draw_plugins(frame, app, inner),
        View::History => draw_history(frame, app, inner),
        View::Library => draw_library(frame, app, inner),
        View::Browse => draw_browse(frame, app, inner),
    }
}

fn draw_search(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(area);

    draw_search_bar(frame, app, rows[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(rows[1].width as usize),
            Style::default().fg(th().border),
        )))
        .style(Style::default().bg(theme::panel2())),
        rows[1],
    );

    if let Some(err) = &app.search_error {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  search failed: {err}"),
                    Style::default().fg(th().red),
                )),
                Line::from(Span::styled(
                    "  press Enter or R to retry, or check the plugin store",
                    Style::default().fg(th().dim),
                )),
            ])
            .style(Style::default().bg(theme::panel2()))
            .wrap(Wrap { trim: true }),
            rows[2],
        );
        return;
    }

    if app.search_loading {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!(
                        "  {} querying the active metadata plugin…",
                        app.spinner_char()
                    ),
                    Style::default().fg(th().yellow),
                )),
                Line::from(Span::styled(
                    "  results will appear here as soon as the plugin responds",
                    Style::default().fg(th().dim),
                )),
            ])
            .style(Style::default().bg(theme::panel2())),
            rows[2],
        );
        return;
    }

    if app.search_results.is_empty() {
        let empty_query = app.search_query.trim().is_empty();
        let mut helper = if empty_query {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Search the streaming universe.",
                    Style::default().fg(th().fg).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Press / to focus the search bar, type an artist or song,",
                    Style::default().fg(th().dim),
                )),
                Line::from(Span::styled(
                    "  then Enter. Tracks and album art come from the metadata",
                    Style::default().fg(th().dim),
                )),
                Line::from(Span::styled(
                    "  plugin; audio is resolved by the streaming plugin.",
                    Style::default().fg(th().dim),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  try  ", Style::default().fg(th().faint)),
                    Span::styled("daft punk", Style::default().fg(th().accent2)),
                    Span::styled("  ·  ", Style::default().fg(th().faint)),
                    Span::styled("radiohead", Style::default().fg(th().accent2)),
                    Span::styled("  ·  ", Style::default().fg(th().faint)),
                    Span::styled("kendrick lamar", Style::default().fg(th().accent2)),
                ]),
            ]
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  No results for “{}”.", app.search_query),
                    Style::default().fg(th().fg),
                )),
                Line::from(Span::styled(
                    "  Try fewer words, a different spelling, or another metadata plugin.",
                    Style::default().fg(th().dim),
                )),
            ]
        };
        if empty_query && !app.recent_queries().is_empty() {
            helper.push(Line::from(""));
            helper.push(Line::from(Span::styled(
                "  recent",
                Style::default().fg(th().faint).add_modifier(Modifier::BOLD),
            )));
            for query in app.recent_queries().iter().take(5) {
                helper.push(Line::from(vec![
                    Span::styled("  ↺ ", Style::default().fg(th().accent)),
                    Span::styled(query.clone(), Style::default().fg(th().dim)),
                ]));
            }
            helper.push(Line::from(""));
            helper.push(Line::from(Span::styled(
                "  ↑ / ↓ in the empty field recalls them",
                Style::default().fg(th().faint),
            )));
        }
        frame.render_widget(
            Paragraph::new(helper)
                .style(Style::default().bg(theme::panel2()))
                .wrap(Wrap { trim: true }),
            rows[2],
        );
        return;
    }

    let now_id = app.now_playing.as_ref().map(|track| track.id.as_str());
    let width = rows[2].width as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut selectable = 0usize;
    let mut selected_line = 0usize;
    let mut hit_rows: Vec<(usize, usize)> = Vec::new();
    for row in app.search_display_rows() {
        match row {
            crate::app::SearchRow::Header(label) => {
                lines.push(section_row(label, width));
            }
            crate::app::SearchRow::Artist(index) => {
                let selected = selectable == app.search_selected;
                if selected {
                    selected_line = lines.len();
                }
                hit_rows.push((selectable, lines.len()));
                if let Some(artist) = app.search_results.artists.get(index) {
                    lines.push(artist_line(artist, selected, width));
                }
                selectable += 1;
            }
            crate::app::SearchRow::Album(index) => {
                let selected = selectable == app.search_selected;
                if selected {
                    selected_line = lines.len();
                }
                hit_rows.push((selectable, lines.len()));
                if let Some(album) = app.search_results.albums.get(index) {
                    lines.push(album_line(album, selected, width));
                }
                selectable += 1;
            }
            crate::app::SearchRow::Track(index) => {
                let selected = selectable == app.search_selected;
                if selected {
                    selected_line = lines.len();
                }
                hit_rows.push((selectable, lines.len()));
                if let Some(track) = app.search_results.tracks.get(index) {
                    lines.push(track_line(
                        track,
                        selected,
                        now_id == Some(track.id.as_str()),
                        app.is_favorite(&track.id),
                        app.marked.contains_key(&track.id),
                        width,
                        Some(index + 1),
                    ));
                }
                selectable += 1;
            }
        }
    }
    let height = rows[2].height as usize;
    let offset = if height > 0 && selected_line >= height {
        (selected_line + 1 - height) as u16
    } else {
        0
    };
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme::panel2()))
            .scroll((offset, 0)),
        rows[2],
    );
    app.record_hits(
        rows[2],
        offset as usize,
        hit_rows
            .into_iter()
            .map(|(index, line)| (line, HitAction::Select(View::Search, index)))
            .collect(),
    );
}

fn section_row(label: &str, width: usize) -> Line<'static> {
    let rule_width = width.saturating_sub(display_width(label) + 6);
    Line::from(vec![
        Span::styled(
            format!("  {} ", label.to_uppercase()),
            Style::default().fg(th().faint).add_modifier(Modifier::BOLD),
        ),
        Span::styled("─".repeat(rule_width), Style::default().fg(th().border)),
    ])
}

fn artist_line(artist: &crate::models::Artist, selected: bool, width: usize) -> Line<'static> {
    let base = if selected {
        Style::default()
            .bg(th().accent)
            .fg(th().bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th().fg)
    };
    let marker_style = if selected {
        Style::default()
            .bg(th().accent)
            .fg(th().bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(th().accent2)
            .add_modifier(Modifier::BOLD)
    };
    let marker = if selected { " ▌ " } else { " ◉ " };
    let right = format!(" artist · {} ", artist.source);
    let right_width = display_width(&right);
    let name_width = width.saturating_sub(right_width + 5);
    let name = truncate(&artist.name, name_width);
    let pad = width
        .saturating_sub(display_width(marker) + display_width(&name) + right_width)
        .max(1);
    Line::from(vec![
        Span::styled(marker, marker_style),
        Span::styled(name, base),
        Span::styled(" ".repeat(pad), base),
        Span::styled(right, Style::default().fg(th().dim)),
    ])
}

fn album_line(album: &crate::models::Album, selected: bool, width: usize) -> Line<'static> {
    let base = if selected {
        Style::default()
            .bg(th().accent)
            .fg(th().bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th().fg)
    };
    let marker_style = if selected {
        Style::default()
            .bg(th().accent)
            .fg(th().bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(th().purple)
            .add_modifier(Modifier::BOLD)
    };
    let marker = if selected { " ▌ " } else { " ▣ " };
    let right = format!(
        " {}{} ",
        album.year.clone().unwrap_or_default(),
        album
            .track_count
            .map(|count| format!(" · {count} tracks"))
            .unwrap_or_default()
    );
    let right_width = display_width(&right);
    let label = if album.artist.trim().is_empty() {
        album.title.clone()
    } else {
        format!("{}  ·  {}", album.title, album.artist)
    };
    let label = truncate(&label, width.saturating_sub(right_width + 5));
    let pad = width
        .saturating_sub(display_width(marker) + display_width(&label) + right_width)
        .max(1);
    Line::from(vec![
        Span::styled(marker, marker_style),
        Span::styled(label, base),
        Span::styled(" ".repeat(pad), base),
        Span::styled(right, Style::default().fg(th().dim)),
    ])
}

fn draw_browse(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(page) = app.browse_page() else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  nothing to browse",
                Style::default().fg(th().dim),
            ))
            .style(Style::default().bg(theme::panel2())),
            area,
        );
        return;
    };

    let hero_height = 3u16.min(area.height);
    let rows = Layout::vertical([Constraint::Length(hero_height), Constraint::Min(1)]).split(area);
    let hero = vec![
        Line::from(Span::styled(
            format!("  {}{}", page.kind_label(), page.title),
            Style::default()
                .fg(th().accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            {
                let mut line = format!(
                    "  {} · {} albums · {} tracks",
                    page.subtitle,
                    page.albums.len(),
                    page.tracks.len()
                );
                if !page.source.is_empty() && !page.subtitle.contains(&page.source) {
                    line.push_str(&format!(" · via {}", page.source));
                }
                line
            },
            Style::default().fg(th().dim),
        )),
        Line::from(Span::styled(
            page.artwork
                .clone()
                .map(|url| truncate(&format!("  art  {url}"), rows[0].width as usize))
                .unwrap_or_else(|| "  no artwork URL".to_string()),
            Style::default().fg(th().faint),
        )),
    ];
    frame.render_widget(
        Paragraph::new(hero).style(Style::default().bg(theme::panel2())),
        rows[0],
    );

    if page.loading {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  {} loading {}…", app.spinner_char(), page.subtitle),
                    Style::default().fg(th().yellow),
                )),
                Line::from(Span::styled(
                    "  Esc goes back to search",
                    Style::default().fg(th().dim),
                )),
            ])
            .style(Style::default().bg(theme::panel2())),
            rows[1],
        );
        return;
    }

    if let Some(err) = &page.error {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  could not load this page: {err}"),
                    Style::default().fg(th().red),
                )),
                Line::from(Span::styled(
                    "  press R to retry or Esc to go back",
                    Style::default().fg(th().dim),
                )),
            ])
            .style(Style::default().bg(theme::panel2())),
            rows[1],
        );
        return;
    }

    let now_id = app.now_playing.as_ref().map(|track| track.id.as_str());
    let width = rows[1].width as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut selectable = 0usize;
    let mut selected_line = 0usize;
    let mut hit_rows: Vec<(usize, usize)> = Vec::new();
    for row in app.browse_display_rows() {
        match row {
            crate::app::BrowseRow::Header(label) => {
                lines.push(section_row(label, width));
            }
            crate::app::BrowseRow::Album(index) => {
                let selected = selectable == page.selected;
                if selected {
                    selected_line = lines.len();
                }
                hit_rows.push((selectable, lines.len()));
                if let Some(album) = page.albums.get(index) {
                    lines.push(album_line(album, selected, width));
                }
                selectable += 1;
            }
            crate::app::BrowseRow::Track(index) => {
                let selected = selectable == page.selected;
                if selected {
                    selected_line = lines.len();
                }
                hit_rows.push((selectable, lines.len()));
                if let Some(track) = page.tracks.get(index) {
                    lines.push(track_line(
                        track,
                        selected,
                        now_id == Some(track.id.as_str()),
                        app.is_favorite(&track.id),
                        app.marked.contains_key(&track.id),
                        width,
                        Some(index + 1),
                    ));
                }
                selectable += 1;
            }
        }
    }
    let height = rows[1].height as usize;
    let offset = if height > 0 && selected_line >= height {
        (selected_line + 1 - height) as u16
    } else {
        0
    };
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme::panel2()))
            .scroll((offset, 0)),
        rows[1],
    );
    app.record_hits(
        rows[1],
        offset as usize,
        hit_rows
            .into_iter()
            .map(|(index, line)| (line, HitAction::Select(View::Browse, index)))
            .collect(),
    );
}

fn draw_search_bar(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.search_focused;
    let border = if focused { th().accent } else { th().border };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let prompt_style = if focused {
        Style::default()
            .fg(th().accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th().dim)
    };
    let text = if app.search_query.is_empty() {
        Span::styled(
            "search songs, artists, albums…",
            Style::default()
                .fg(th().faint)
                .add_modifier(Modifier::ITALIC),
        )
    } else {
        Span::styled(app.search_query.clone(), Style::default().fg(th().fg))
    };
    let cursor = if focused {
        Span::styled("▌", Style::default().fg(th().accent))
    } else {
        Span::raw("")
    };
    let hint = if focused {
        " Enter search/play · ↑↓ history · Esc clear/unfocus "
    } else if app.search_query.trim().is_empty() {
        " press / to type "
    } else {
        " / edit · Enter play "
    };
    let line = Line::from(vec![
        Span::styled(" ⌕  ", prompt_style),
        text,
        cursor,
        Span::styled(hint, Style::default().fg(th().faint)),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme::panel())),
        inner,
    );
}

fn draw_queue(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.queue.is_empty() {
        draw_empty(
            frame,
            area,
            "The queue is empty.",
            "Search for a song and press Enter to play, or press a to queue it.",
        );
        return;
    }
    let now_id = app.now_playing.as_ref().map(|track| track.id.as_str());
    let inner = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
    let hint = Line::from(vec![
        Span::styled(" playing ", Style::default().fg(th().faint)),
        Span::styled("▶", Style::default().fg(th().green)),
        Span::styled("  current ", Style::default().fg(th().faint)),
        Span::styled("J/K", Style::default().fg(th().accent2)),
        Span::styled(" reorder · ", Style::default().fg(th().dim)),
        Span::styled("x", Style::default().fg(th().accent2)),
        Span::styled(" remove · ", Style::default().fg(th().dim)),
        Span::styled("n/b", Style::default().fg(th().accent2)),
        Span::styled(" skip", Style::default().fg(th().dim)),
    ]);
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().bg(theme::panel2())),
        inner[0],
    );
    let hits = render_track_list(
        frame,
        inner[1],
        &app.queue,
        app.queue_selected,
        now_id,
        |track| app.is_favorite(&track.id),
        |track| app.marked.contains_key(&track.id),
        false,
    );
    app.record_hits(
        inner[1],
        0,
        hits.into_iter()
            .map(|(index, line)| (line, HitAction::Select(View::Queue, index)))
            .collect(),
    );
}

fn draw_favorites(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.favorites.is_empty() {
        draw_empty(
            frame,
            area,
            "No favorites yet.",
            "Find a track and press F. Favorites persist between sessions.",
        );
        return;
    }
    let now_id = app.now_playing.as_ref().map(|track| track.id.as_str());
    let hits = render_track_list(
        frame,
        area,
        &app.favorites,
        app.favorites_selected,
        now_id,
        |_| true,
        |track| app.marked.contains_key(&track.id),
        true,
    );
    app.record_hits(
        area,
        0,
        hits.into_iter()
            .map(|(index, line)| (line, HitAction::Select(View::Favorites, index)))
            .collect(),
    );
}

fn draw_playlists(frame: &mut Frame, app: &mut App, area: Rect) {
    if let Some(index) = app.open_playlist {
        let Some(playlist) = app.playlists.get(index) else {
            draw_empty(frame, area, "Playlist not found.", "Esc to go back.");
            return;
        };
        if playlist.tracks.is_empty() {
            draw_empty(
                frame,
                area,
                &format!("“{}” is empty.", playlist.name),
                "Add tracks with P from search, queue, or favorites.",
            );
            return;
        }
        let now_id = app.now_playing.as_ref().map(|track| track.id.as_str());
        let hits = render_track_list(
            frame,
            area,
            &playlist.tracks,
            app.playlist_track_selected,
            now_id,
            |track| app.is_favorite(&track.id),
            |track| app.marked.contains_key(&track.id),
            false,
        );
        app.record_hits(
            area,
            0,
            hits.into_iter()
                .map(|(index, line)| (line, HitAction::Select(View::Playlists, index)))
                .collect(),
        );
        return;
    }

    if app.playlists.is_empty() {
        draw_empty(
            frame,
            area,
            "No playlists yet.",
            "Press c to create one, then add tracks with P.",
        );
        return;
    }

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Enter", Style::default().fg(th().accent2)),
            Span::styled(" open · ", Style::default().fg(th().dim)),
            Span::styled("p", Style::default().fg(th().accent2)),
            Span::styled(" play · ", Style::default().fg(th().dim)),
            Span::styled("c", Style::default().fg(th().accent2)),
            Span::styled(" new · ", Style::default().fg(th().dim)),
            Span::styled("d", Style::default().fg(th().accent2)),
            Span::styled(" delete", Style::default().fg(th().dim)),
        ]))
        .style(Style::default().bg(theme::panel2())),
        rows[0],
    );
    render_playlist_list(frame, app, rows[1]);
}

fn render_playlist_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let (start, end) = window(
        app.playlists.len(),
        app.playlist_selected,
        area.height as usize,
    );
    let width = area.width as usize;
    let mut lines = Vec::new();
    let mut hits = Vec::new();
    for index in start..end {
        hits.push((index - start, HitAction::Select(View::Playlists, index)));
        let playlist = &app.playlists[index];
        let selected = index == app.playlist_selected;
        let style = if selected {
            Style::default()
                .bg(th().accent)
                .fg(th().bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(th().fg)
        };
        let bar = if selected { "▌" } else { " " };
        let left = format!("{} ♫ {}", bar, playlist.name);
        let right = format!(
            "{} tracks · {} ",
            playlist.tracks.len(),
            format_duration(playlist.duration())
        );
        let right_width = display_width(&right);
        let left = truncate(&left, width.saturating_sub(right_width + 1));
        let pad = width
            .saturating_sub(display_width(&left) + right_width)
            .max(1);
        lines.push(Line::from(vec![
            Span::styled(left, style),
            Span::styled(" ".repeat(pad), style),
            Span::styled(right, style),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel2())),
        area,
    );
    app.record_hits(area, 0, hits);
}

fn draw_downloads(frame: &mut Frame, app: &mut App, area: Rect) {
    let tracks = app.download_tracks();
    if tracks.is_empty() {
        draw_empty(
            frame,
            area,
            "No offline downloads yet.",
            "Pick a song and press d. Downloads play instantly and survive bad networks.",
        );
        return;
    }
    let now_id = app.now_playing.as_ref().map(|track| track.id.as_str());
    let hits = render_track_list(
        frame,
        area,
        &tracks,
        app.downloads_selected,
        now_id,
        |track| app.is_favorite(&track.id),
        |track| app.marked.contains_key(&track.id),
        true,
    );
    app.record_hits(
        area,
        0,
        hits.into_iter()
            .map(|(index, line)| (line, HitAction::Select(View::Downloads, index)))
            .collect(),
    );
}

fn draw_library(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.library_scanning && app.library.is_empty() {
        draw_empty(
            frame,
            area,
            "Scanning your local library…",
            "Reading tags and embedded artwork. Large collections take a moment.",
        );
        return;
    }
    if app.library.is_empty() {
        draw_empty(
            frame,
            area,
            "No local music indexed.",
            "Add folders to library_paths in config.toml, then press R to scan.",
        );
        return;
    }
    let now_id = app.now_playing.as_ref().map(|track| track.id.as_str());
    let hits = render_track_list(
        frame,
        area,
        &app.library,
        app.library_selected,
        now_id,
        |track| app.is_favorite(&track.id),
        |track| app.marked.contains_key(&track.id),
        false,
    );
    app.record_hits(
        area,
        0,
        hits.into_iter()
            .map(|(index, line)| (line, HitAction::Select(View::Library, index)))
            .collect(),
    );
}

fn draw_history(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.history.is_empty() {
        draw_empty(
            frame,
            area,
            "No listening history yet.",
            "Play something. Every track you start shows up here, newest first.",
        );
        return;
    }
    let tracks = app.history_tracks();
    let now_id = app.now_playing.as_ref().map(|track| track.id.as_str());
    let hits = render_track_list(
        frame,
        area,
        &tracks,
        app.history_selected,
        now_id,
        |track| app.is_favorite(&track.id),
        |track| app.marked.contains_key(&track.id),
        false,
    );
    app.record_hits(
        area,
        0,
        hits.into_iter()
            .map(|(index, line)| (line, HitAction::Select(View::History, index)))
            .collect(),
    );
}

fn draw_stats(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_fixed(area, 58, 18);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().accent))
        .title(Span::styled(
            " listening stats ",
            Style::default()
                .fg(th().accent)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let plays = app.history.len();
    let mut seconds = 0u64;
    let mut artists: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut tracks: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for entry in &app.history {
        seconds += entry.track.duration.unwrap_or(0);
        if !entry.track.artist.trim().is_empty() {
            *artists.entry(entry.track.artist.clone()).or_default() += 1;
        }
        *tracks.entry(entry.track.title.clone()).or_default() += 1;
    }
    let mut top_artists: Vec<_> = artists.into_iter().collect();
    top_artists.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut top_tracks: Vec<_> = tracks.into_iter().collect();
    top_tracks.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut lines = vec![
        Line::from(vec![
            Span::styled(" plays ", Style::default().fg(th().dim)),
            Span::styled(
                plays.to_string(),
                Style::default()
                    .fg(th().accent2)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ·  listened ", Style::default().fg(th().dim)),
            Span::styled(
                format_duration(seconds),
                Style::default()
                    .fg(th().accent2)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " top artists",
            Style::default().fg(th().faint),
        )),
    ];
    if top_artists.is_empty() {
        lines.push(Line::from(Span::styled(
            "  nothing yet",
            Style::default().fg(th().dim),
        )));
    }
    for (index, (name, count)) in top_artists.iter().take(5).enumerate() {
        lines.push(Line::from(vec![
            Span::styled(format!(" {}. ", index + 1), Style::default().fg(th().faint)),
            Span::styled(truncate(name, 34), Style::default().fg(th().fg)),
            Span::styled(format!("  {count}×"), Style::default().fg(th().dim)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " most played",
        Style::default().fg(th().faint),
    )));
    for (index, (name, count)) in top_tracks.iter().take(4).enumerate() {
        lines.push(Line::from(vec![
            Span::styled(format!(" {}. ", index + 1), Style::default().fg(th().faint)),
            Span::styled(truncate(name, 34), Style::default().fg(th().fg)),
            Span::styled(format!("  {count}×"), Style::default().fg(th().dim)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " any key to close",
        Style::default().fg(th().faint),
    )));
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel())),
        inner,
    );
}

#[allow(clippy::needless_range_loop)]
fn draw_plugins(frame: &mut Frame, app: &mut App, area: Rect) {
    let entries = app.plugin_entries();
    if entries.is_empty() {
        draw_empty(frame, area, "No plugins.", "The bundled store is empty.");
        return;
    }
    let (start, end) = window(entries.len(), app.plugin_selected, area.height as usize);
    let width = area.width as usize;
    let mut lines = Vec::new();
    let mut hits = Vec::new();
    for index in start..end {
        hits.push((index - start, HitAction::Select(View::Plugins, index)));
        let entry = &entries[index];
        let selected = index == app.plugin_selected;
        let base = if selected {
            Style::default().bg(th().accent).fg(th().bg)
        } else {
            Style::default().fg(th().fg)
        };
        let kind_style = if selected {
            Style::default()
                .bg(th().accent)
                .fg(th().bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(match entry.kind {
                PluginKind::Metadata => th().accent2,
                PluginKind::Streaming => th().green,
                PluginKind::Lyrics => th().purple,
            })
        };
        let state = if entry.active {
            "● active"
        } else if entry.installed {
            "installed"
        } else {
            "available"
        };
        let state_style = if selected {
            Style::default()
                .bg(th().accent)
                .fg(th().bg)
                .add_modifier(Modifier::BOLD)
        } else if entry.active {
            Style::default().fg(th().green).add_modifier(Modifier::BOLD)
        } else if entry.installed {
            Style::default().fg(th().dim)
        } else {
            Style::default().fg(th().yellow)
        };
        let marker = if selected { "▌" } else { " " };
        let left = format!("{} {} {}  v{}", marker, entry.name, "", entry.version);
        let kind_label = format!("{:<6}", entry.kind.badge());
        let state_label = format!("{:<10} ", state);
        let right_width = 6 + display_width(&state_label) + 1;
        let left_width = width.saturating_sub(right_width + 2);
        let left = truncate(&left, left_width);
        let pad = width
            .saturating_sub(display_width(&left) + right_width)
            .max(1);
        lines.push(Line::from(vec![
            Span::styled(left, base.add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(pad), base),
            Span::styled(kind_label, kind_style),
            Span::styled(" ", base),
            Span::styled(state_label, state_style),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel2())),
        area,
    );
    app.record_hits(area, 0, hits);
}

fn draw_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().border))
        .title(Span::styled(" inspect ", Style::default().fg(th().dim)))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.view == View::Plugins {
        let entries = app.plugin_entries();
        if let Some(entry) = entries.get(app.plugin_selected) {
            let lines = vec![
                Line::from(Span::styled(
                    entry.name.clone(),
                    Style::default()
                        .fg(th().accent)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("{} · v{}", entry.kind.label(), entry.version),
                    Style::default().fg(th().accent2),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("status  ", Style::default().fg(th().faint)),
                    Span::styled(
                        if entry.active {
                            "active"
                        } else if entry.installed {
                            "installed"
                        } else {
                            "available"
                        },
                        Style::default().fg(if entry.active { th().green } else { th().dim }),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("source  ", Style::default().fg(th().faint)),
                    Span::styled(
                        if entry.bundled {
                            "bundled store"
                        } else {
                            "user plugin"
                        },
                        Style::default().fg(th().dim),
                    ),
                ]),
                Line::from(""),
                Line::from(Span::styled(
                    entry.description.clone(),
                    Style::default().fg(th().dim),
                )),
                Line::from(""),
                action_line("Enter", "activate / install"),
                action_line("i", "install"),
                action_line("u", "uninstall"),
                action_line("R", "rescan plugins"),
            ];
            frame.render_widget(
                Paragraph::new(lines)
                    .style(Style::default().bg(theme::panel()))
                    .wrap(Wrap { trim: true }),
                inner,
            );
        }
        return;
    }

    let selected = app.selected_track().or_else(|| app.now_playing.clone());
    let art = app.artwork.as_ref().filter(|_| {
        selected
            .as_ref()
            .is_some_and(|track| app.artwork_track_id.as_deref() == Some(track.id.as_str()))
    });
    let art_height = if art.is_some() && inner.width >= 14 {
        (inner.width / 2).min(inner.height / 2).max(4)
    } else {
        0
    };
    let sections =
        Layout::vertical([Constraint::Length(art_height), Constraint::Min(0)]).split(inner);
    if let Some(art) = art {
        draw_art(frame, art, sections[0]);
    }
    let body = sections[1];

    let Some(track) = selected else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  nothing selected",
                    Style::default()
                        .fg(th().faint)
                        .add_modifier(Modifier::ITALIC),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Pick a track to see its metadata,\n  artwork URL, source and shortcuts.",
                    Style::default().fg(th().dim),
                )),
            ])
            .style(Style::default().bg(theme::panel()))
            .wrap(Wrap { trim: true }),
            body,
        );
        return;
    };

    let favorite = app.is_favorite(&track.id);
    let mut lines = vec![
        Line::from(Span::styled(
            "SELECTED",
            Style::default().fg(th().faint).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            track.title.clone(),
            Style::default()
                .fg(th().accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            track.artist.clone(),
            Style::default().fg(th().fg),
        )),
        Line::from(Span::styled(
            track.album.clone(),
            Style::default().fg(th().dim).add_modifier(Modifier::ITALIC),
        )),
        Line::from(""),
    ];
    lines.push(info_line("duration", track.duration_label()));
    lines.push(info_line("source", track.source.clone()));
    if let Some(year) = &track.year {
        lines.push(info_line("year", year.clone()));
    }
    lines.push(info_line(
        "favorite",
        if favorite { "★ yes" } else { "☆ no" }.to_string(),
    ));
    lines.push(info_line(
        "offline",
        if app.downloads.contains_key(&track.id) {
            "⤓ downloaded"
        } else {
            "not downloaded"
        }
        .to_string(),
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "ARTWORK",
        Style::default().fg(th().faint).add_modifier(Modifier::BOLD),
    )));
    if let Some(artwork) = &track.artwork {
        lines.push(Line::from(Span::styled(
            artwork.clone(),
            Style::default().fg(th().accent2),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "no artwork URL from provider",
            Style::default().fg(th().faint),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "ACTIONS",
        Style::default().fg(th().faint).add_modifier(Modifier::BOLD),
    )));
    lines.push(action_line("Enter", "play"));
    lines.push(action_line("a", "queue"));
    lines.push(action_line("F", "favorite"));
    lines.push(action_line("P", "add to playlist"));
    lines.push(action_line("d", "download offline"));
    lines.push(action_line("v", "mark for bulk actions"));
    lines.push(action_line("y", "lyrics"));
    lines.push(action_line("Space", "pause / resume"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "AUDIO",
        Style::default().fg(th().faint).add_modifier(Modifier::BOLD),
    )));
    lines.push(info_line("eq", app.eq.clone()));
    lines.push(info_line(
        "normalise",
        if app.normalize { "on" } else { "off" }.to_string(),
    ));
    lines.push(info_line(
        "radio",
        if app.radio { "on" } else { "off" }.to_string(),
    ));
    lines.push(info_line(
        "fade",
        if app.fade_seconds() > 0.0 {
            format!("{:.1}s", app.fade_seconds())
        } else {
            "off".to_string()
        },
    ));
    lines.push(info_line("replaygain", app.replaygain_mode().to_string()));
    lines.push(info_line(
        "scrobble",
        if app.scrobbling() {
            "listenbrainz"
        } else {
            "off"
        }
        .to_string(),
    ));
    lines.push(info_line(
        "cava",
        if app.cava_enabled { "on" } else { "off" }.to_string(),
    ));
    if let Some(sleep) = app.sleep_label() {
        lines.push(info_line("sleep", sleep));
    }
    if !app.marked.is_empty() {
        lines.push(info_line("marked", format!("{} tracks", app.marked.len())));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme::panel()))
            .wrap(Wrap { trim: true }),
        body,
    );
}

fn draw_art(frame: &mut Frame, art: &crate::artwork::Artwork, area: Rect) {
    if area.width < 4 || area.height < 2 {
        return;
    }
    let cell_h = area.height.min((area.width / 2).max(1));
    let cell_w = (cell_h * 2).min(area.width);
    let x_off = area.x + (area.width.saturating_sub(cell_w)) / 2;
    let mut lines = Vec::with_capacity(cell_h as usize);
    for row in 0..cell_h {
        let mut spans = Vec::with_capacity(cell_w as usize);
        let y0 = ((row as f32 / cell_h as f32) * art.height as f32) as usize;
        let y1 = (((row as f32 + 1.0) / cell_h as f32) * art.height as f32) as usize;
        for col in 0..cell_w {
            let x = ((col as f32 / cell_w as f32) * art.width as f32) as usize;
            let top = art.pixel(x, y0.min(art.height - 1));
            let bottom = art.pixel(x, y1.min(art.height - 1));
            spans.push(Span::styled(
                "▀",
                Style::default()
                    .fg(ratatui::style::Color::Rgb(top[0], top[1], top[2]))
                    .bg(ratatui::style::Color::Rgb(bottom[0], bottom[1], bottom[2])),
            ));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel())),
        Rect {
            x: x_off,
            y: area.y,
            width: cell_w,
            height: cell_h,
        },
    );
}

fn draw_eq(frame: &mut Frame, app: &App, selected: usize, area: Rect) {
    let popup = centered_fixed(area, 76, 24);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().accent))
        .title(Span::styled(
            " equaliser · ctrl+e ",
            Style::default()
                .fg(th().accent)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines = vec![
        Line::from(Span::styled(
            " ←/→ pick band · ↑/↓ ±0.5 dB · Enter apply · r flat · Esc close",
            Style::default().fg(th().faint),
        )),
        Line::from(""),
    ];
    for (index, gain) in app.eq_gains.iter().enumerate() {
        let active = index == selected;
        let start = if active { "▌" } else { " " };
        let span = *gain;
        let bars = (span.abs() * 2.0) as usize;
        let bar: String = if span >= 0.0 {
            format!("{}{}", "█".repeat(bars), "_".repeat(12 - bars.min(12)))
        } else {
            format!(
                "{}{}",
                "_".repeat(12 - bars.min(12)),
                "░".repeat(bars.min(12))
            )
        };
        let label = format!("{:6}", crate::app::EQ_BANDS[index]);
        lines.push(Line::from(vec![
            Span::styled(format!("{start} "), Style::default().fg(th().accent)),
            Span::styled(
                label,
                Style::default().fg(if active { th().accent } else { th().dim }),
            ),
            Span::styled(
                format!(" {bar} {span:+5.1} dB"),
                Style::default().fg(if active { th().fg } else { th().faint }),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(" profile: {}", app.eq),
        Style::default().fg(th().accent2),
    )));
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel())),
        inner,
    );
}

/// A smooth truecolor ramp across a piece of text. Falls back to the start
/// colour on terminals that only speak ANSI.
fn gradient_text(text: &str, from: Color, to: Color, modifier: Modifier) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let last = chars.len().saturating_sub(1).max(1) as f32;
    chars
        .iter()
        .enumerate()
        .map(|(index, ch)| {
            let t = index as f32 / last;
            Span::styled(
                ch.to_string(),
                Style::default().fg(mix(from, to, t)).add_modifier(modifier),
            )
        })
        .collect()
}

fn mix(from: Color, to: Color, t: f32) -> Color {
    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let t = t.clamp(0.0, 1.0);
            let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
            Color::Rgb(channel(r1, r2), channel(g1, g2), channel(b1, b2))
        }
        _ => from,
    }
}

/// Eighth-block progress bar: smooth head, gradient fill, quiet remainder.
fn bar_spans(width: usize, ratio: f64) -> Vec<Span<'static>> {
    const PARTIAL: [&str; 9] = [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];
    let ratio = ratio.clamp(0.0, 1.0);
    let cells = ratio * width as f64;
    let full = cells.floor() as usize;
    let partial = ((cells - full as f64) * 8.0).round() as usize;
    let denominator = width.max(1) as f32;

    (0..width)
        .map(|index| {
            let colour = mix(th().accent, th().accent2, index as f32 / denominator);
            if index < full {
                Span::styled("█", Style::default().fg(colour))
            } else if index == full && partial > 0 {
                Span::styled(PARTIAL[partial.min(8)], Style::default().fg(colour))
            } else {
                Span::styled("░", Style::default().fg(th().border))
            }
        })
        .collect()
}

fn info_line(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9}"), Style::default().fg(th().faint)),
        Span::styled(value, Style::default().fg(th().fg)),
    ])
}

fn action_line(key: &str, action: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<9}"), Style::default().fg(th().accent2)),
        Span::styled(action.to_string(), Style::default().fg(th().dim)),
    ])
}

fn draw_player(frame: &mut Frame, app: &mut App, area: Rect) {
    let snapshot = app.audio_snapshot();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().border))
        .title(Line::from(vec![
            Span::styled(" now playing ", Style::default().fg(th().dim)),
            Span::styled(
                if app.resolving {
                    format!("{} resolving ", app.spinner_char())
                } else if app.now_playing.is_some() {
                    "● ".to_string()
                } else {
                    "○ ".to_string()
                },
                Style::default().fg(if app.resolving {
                    th().yellow
                } else if app.now_playing.is_some() {
                    th().green
                } else {
                    th().faint
                }),
            ),
        ]))
        .style(Style::default().bg(theme::panel2()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);

    let (title, artist) = if let Some(track) = &app.now_playing {
        let offline = app.downloads.contains_key(&track.id);
        (
            format!(
                "{}{}",
                track.title,
                if offline { "  ⤓ offline" } else { "" }
            ),
            track.artist.clone(),
        )
    } else {
        (
            "nothing playing".to_string(),
            "press / to search, Enter to play".to_string(),
        )
    };
    let left_width = rows[0].width.saturating_sub(16) as usize;
    let line = Line::from(vec![
        Span::styled(" ▶ ", Style::default().fg(th().accent)),
        Span::styled(
            truncate(&title, left_width.saturating_sub(2)),
            Style::default().fg(th().fg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", truncate(&artist, 30)),
            Style::default().fg(th().dim),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme::panel2())),
        rows[0],
    );

    let wide = inner.width >= 64;
    let gauge_area = if wide {
        Layout::horizontal([Constraint::Min(20), Constraint::Length(34)]).split(rows[1])
    } else {
        Layout::horizontal([Constraint::Min(10)]).split(rows[1])
    };
    let duration = snapshot.duration.max(
        app.now_playing
            .as_ref()
            .and_then(|t| t.duration)
            .map(|d| d as f64)
            .unwrap_or(0.0),
    );
    let ratio = if duration > 0.0 {
        (snapshot.position / duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let time_label = if duration > 0.0 {
        format!(
            " {} / {} ",
            format_position(snapshot.position),
            format_position(duration)
        )
    } else {
        format!(" {} / --:-- ", format_position(snapshot.position))
    };
    let bar_width = (gauge_area[0].width as usize)
        .saturating_sub(time_label.chars().count() + 1)
        .max(1);
    let mut progress = Line::from(bar_spans(bar_width, ratio));
    progress.spans.push(Span::styled(
        time_label,
        Style::default().fg(th().dim).bg(theme::panel()),
    ));
    frame.render_widget(
        Paragraph::new(progress).style(Style::default().bg(theme::panel())),
        gauge_area[0],
    );
    app.record_hits(gauge_area[0], 0, vec![(0, HitAction::SeekBar)]);

    let volume_label = if app.volume <= 0.0 {
        "muted".to_string()
    } else {
        format!("vol {:.0}%", app.audio_snapshot().volume.max(app.volume))
    };
    let mut mode = format!(
        "{}  {}  {}{}",
        volume_label,
        if app.shuffle {
            "shuffle"
        } else {
            "shuffle off"
        },
        app.repeat.label(),
        if snapshot.paused { "  paused" } else { "" }
    );
    if app.radio {
        mode.push_str("  radio");
    }
    if app.eq != "flat" {
        mode.push_str(&format!("  eq:{}", app.eq));
    }
    if app.normalize {
        mode.push_str("  norm");
    }
    if let Some(sleep) = app.sleep_label() {
        mode.push_str(&format!("  {sleep}"));
    }
    if wide {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate(&mode, gauge_area[1].width as usize),
                Style::default().fg(if snapshot.paused {
                    th().yellow
                } else {
                    th().dim
                }),
            )))
            .alignment(Alignment::Right)
            .style(Style::default().bg(theme::panel2())),
            gauge_area[1],
        );
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let hints = match app.view {
        View::Search => {
            "/ search · Enter play/open · a queue · v mark · d download · Y share · Ctrl+L import · Ctrl+T radio · ?"
        }
        View::Queue => {
            "/ filter · Enter play · t now playing · J/K reorder · x remove · Alt+U undo · Alt+K clear upcoming · n/b skip · ?"
        }
        View::Favorites => {
            "/ filter · Enter play · a queue · v mark · x remove · F favorite · y lyrics · z sleep · ? help"
        }
        View::Playlists => {
            if app.open_playlist.is_some() {
                "Esc/h back · Enter play · J/K reorder · P add · x remove · / filter · c new · e rename · ?"
            } else {
                "Enter open · p play · c new · e rename · d delete · S save queue · Alt+E export · Alt+I import · ?"
            }
        }
        View::Downloads => {
            "Enter play · d download · x delete · a queue · v mark · / search · ? help · q quit"
        }
        View::Plugins => {
            "Enter activate/install · i install · u uninstall · R rescan · ? help · q quit"
        }
        View::History => {
            "Enter replay · a queue · v mark · Ctrl+G stats · 7 history · ? help · q quit"
        }
        View::Library => {
            "Enter play · a queue · A queue all · v mark · / filter · R rescan · 8 library · ? help"
        }
        View::Browse => {
            "Esc/h back · Enter open/play · p play all · a/A queue · d download · v mark · N next · l like · y lyrics · R reload"
        }
    };
    let cols =
        Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                " {}",
                truncate(hints, cols[0].width.saturating_sub(2) as usize)
            ),
            Style::default().fg(th().faint),
        )))
        .style(Style::default().bg(theme::bg())),
        cols[0],
    );
    let status = if let Some((toast, _)) = &app.toast {
        toast.as_str()
    } else {
        app.status.as_str()
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                "{} ",
                truncate(status, cols[1].width.saturating_sub(2) as usize)
            ),
            Style::default().fg(if app.resolving { th().yellow } else { th().dim }),
        )))
        .alignment(Alignment::Right)
        .style(Style::default().bg(theme::bg())),
        cols[1],
    );
}

fn draw_overlay(frame: &mut Frame, app: &mut App, area: Rect) {
    match app.overlay.as_ref() {
        Some(Overlay::Help) => draw_help(frame, area),
        Some(Overlay::CommandPalette { query, selected }) => {
            draw_palette(frame, query, *selected, area)
        }
        Some(Overlay::Filter {
            kind,
            query,
            selected,
        }) => {
            let kind = *kind;
            let query = query.clone();
            let selected = *selected;
            draw_filter(frame, app, kind, &query, selected, area);
        }
        Some(Overlay::PlaylistPicker { selected }) => {
            draw_playlist_picker(frame, app, *selected, area)
        }
        Some(Overlay::NewPlaylist { name }) => draw_new_playlist(frame, name, area),
        Some(Overlay::RenamePlaylist { name, .. }) => draw_rename_playlist(frame, name, area),
        Some(Overlay::Confirm { message, .. }) => draw_confirm(frame, message, area),
        Some(Overlay::Sleep { selected }) => draw_sleep(frame, app, *selected, area),
        Some(Overlay::ImportLink { input }) => draw_import_link(frame, input, area),
        Some(Overlay::Stats) => draw_stats(frame, app, area),
        Some(Overlay::Eq { selected }) => draw_eq(frame, app, *selected, area),
        Some(Overlay::Lyrics(_)) => draw_lyrics(frame, app, area),
        None => {}
    }
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let popup = centered(area, 78, 90);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().accent))
        .title(Span::styled(
            " keyboard help · any key to close ",
            Style::default()
                .fg(th().accent)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);
    let left = vec![
        section("NAVIGATION"),
        key("Tab / Shift+Tab", "next / previous view"),
        key(
            "1 – 6",
            "search · queue · likes · lists · downloads · plugins",
        ),
        key("j / k, ↑ / ↓", "move selection · wheel scrolls"),
        key("g / G", "top / bottom"),
        key("Enter / i", "play · open artist / album · click row twice"),
        key("h / Alt+←", "go back (page, playlist)"),
        key("/ or Ctrl+F", "search · fuzzy filter current view"),
        key("Esc", "clear search / go back / unfocus"),
        Line::from(""),
        section("SPOTIFY-STYLE ACTIONS"),
        key("a / A", "queue selected / everything in the view"),
        key("N", "play selected next (queue jump)"),
        key("v / V", "mark one / mark all in view"),
        key("F or l", "like / unlike under cursor"),
        key("P", "add selection (or marks) to a playlist"),
        key("J / K", "move track up / down (queue, playlist)"),
        key("S", "save the queue as a playlist"),
        key("Alt+K", "clear upcoming tracks"),
        key("Alt+U", "undo the last removal"),
        key("Alt+E / Alt+I", "export / import playlists (M3U)"),
        Line::from(""),
        section("PLAYBACK"),
        key("Space", "play / pause (starts a restored queue)"),
        key("n / b", "next / previous track"),
        key("Ctrl+← / →", "previous / next track"),
        key("← / →", "seek -5s / +5s"),
        key("Shift+← / →", "seek -30s / +30s"),
        key("+ / - · m", "volume up / down · mute"),
        key("[ / ] · \\", "speed down / up · reset to 1×"),
        key("s · r", "shuffle · cycle repeat"),
        key("z", "sleep timer (15–90 min or end of track, fades out)"),
        key("y", "lyrics overlay (synced when available)"),
        key("R", "reload / retry (page, search, plugins)"),
        key(
            "q / Ctrl+B",
            "quit & stop / quit & keep playing (queue+repeat continue)",
        ),
    ];
    let right = vec![
        section("SMART SEARCH"),
        key("1, /", "songs, artists and albums in one list"),
        key("Enter", "open artist / album, or start playback"),
        key("↑ / ↓ (empty)", "recall recent searches"),
        key("Esc", "clear the field, then unfocus"),
        Line::from(""),
        section("OFFLINE & RADIO"),
        key("d", "download the selection for offline playback"),
        key("5", "downloads view · Enter plays the local file"),
        key("7", "listening history · Enter replays a track"),
        key("8", "local library · Enter plays your own files"),
        key("Y", "share now playing to the clipboard"),
        key("R", "rescan library folders after adding music"),
        key("Ctrl+G", "listening stats: plays, time, top artists"),
        key("Ctrl+T", "radio: keep finding related tracks"),
        key("Ctrl+L", "import songs / a playlist from a link"),
        Line::from(""),
        section("AUDIO & VISUALISER"),
        key("E", "cycle EQ: flat / bass / vocal / night / loud / custom"),
        key("Ctrl+E", "edit the 10-band EQ: ←/→ band, ↑/↓ ±0.5 dB, Enter"),
        key("Alt+F", "smart fade: 0 / 2 / 4 / 6 s at each end of a track"),
        key("Alt+R", "replaygain: off / track / album for tagged files"),
        key("Alt+N", "toggle loudness normalisation"),
        key("Ctrl+V", "toggle the cava spectrum panel"),
        key("inspector", "album art loads automatically for the playing track"),
        key("Alt+T", "cycle colour theme (wallpaper = sample the desktop)"),
        key("Alt+W", "transparent background (let the terminal show the wallpaper)"),
        key("Alt+A", "ambient tint (accent follows the current cover)"),
        key("Ctrl+P / :", "fzf command palette (every action)"),
        key("playerctl", "media keys control this player via MPRIS"),
        Line::from(""),
        section("PLAYBACK RESILIENCE"),
        Line::from(Span::styled(
            " Resolve validates the URL and keeps a backup; on\n failure the player retries, then walks down to the\n next streaming plugin so the queue keeps moving.\n Prefetch resolves the next song while this one plays.",
            Style::default().fg(th().dim),
        )),
        Line::from(""),
        section("PLUGINS"),
        Line::from(Span::styled(
            " Metadata, streaming and lyrics are separate plugins.\n Install or switch them in the plugin store (5).",
            Style::default().fg(th().dim),
        )),
        Line::from(""),
        Line::from(Span::styled(
            " Themes & steps live in ~/.config/mfe-player/config.toml",
            Style::default().fg(th().faint),
        )),
    ];
    frame.render_widget(
        Paragraph::new(left)
            .style(Style::default().bg(theme::panel()))
            .wrap(Wrap { trim: true }),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(right)
            .style(Style::default().bg(theme::panel()))
            .wrap(Wrap { trim: true }),
        columns[1],
    );
}

fn section(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(th().accent2)
            .add_modifier(Modifier::BOLD),
    ))
}

fn key(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {key:<16}"), Style::default().fg(th().yellow)),
        Span::styled(desc.to_string(), Style::default().fg(th().dim)),
    ])
}

fn draw_palette(frame: &mut Frame, query: &str, selected: usize, area: Rect) {
    let popup = centered(area, 72, 64);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().accent))
        .title(Span::styled(
            " command palette · fuzzy · Enter run · Esc close ",
            Style::default()
                .fg(th().accent)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ❯ ", Style::default().fg(th().accent)),
            Span::styled(query.to_string(), Style::default().fg(th().fg)),
            Span::styled("▌", Style::default().fg(th().accent)),
        ]))
        .style(Style::default().bg(theme::panel2())),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(rows[1].width as usize),
            Style::default().fg(th().border),
        ))),
        rows[1],
    );

    let ranked = palette_ranked(query);
    let lines = palette_lines(
        &ranked,
        selected,
        rows[2].width as usize,
        rows[2].height as usize,
    );
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel())),
        rows[2],
    );
}

fn palette_lines(
    ranked: &[usize],
    selected: usize,
    width: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let (start, end) = window(ranked.len(), selected, height);
    let mut lines = Vec::new();
    for position in start..end {
        let Some(index) = ranked.get(position) else {
            continue;
        };
        let item: &PaletteItem = &PALETTE_ITEMS[*index];
        let active = position == selected;
        let style = if active {
            Style::default()
                .bg(th().accent)
                .fg(th().bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(th().fg)
        };
        let hint_style = if active {
            Style::default().bg(th().accent).fg(th().bg)
        } else {
            Style::default().fg(th().faint)
        };
        let hint = format!("{:<10}", item.hint);
        let label_width = width.saturating_sub(display_width(&hint) + 3);
        let label = truncate(item.label, label_width);
        lines.push(Line::from(vec![
            Span::styled(if active { " ▌ " } else { "   " }, style),
            Span::styled(label, style),
            Span::styled(
                " ".repeat(width.saturating_sub(display_width(&hint) + 2)),
                style,
            ),
            Span::styled(hint, hint_style),
        ]));
    }
    lines
}

fn draw_filter(
    frame: &mut Frame,
    app: &App,
    kind: FilterKind,
    query: &str,
    selected: usize,
    area: Rect,
) {
    let context = match kind {
        FilterKind::Search => "search results",
        FilterKind::Queue => "queue",
        FilterKind::Favorites => "favorites",
        FilterKind::Playlist(index) => app
            .playlists
            .get(index)
            .map(|playlist| playlist.name.as_str())
            .unwrap_or("playlist"),
        FilterKind::Library => "local library",
    };
    let popup = centered(area, 74, 72);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().accent2))
        .title(Span::styled(
            format!(" fuzzy filter · {context} · Enter play · Esc close "),
            Style::default()
                .fg(th().accent2)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ❯ ", Style::default().fg(th().accent2)),
            Span::styled(query.to_string(), Style::default().fg(th().fg)),
            Span::styled("▌", Style::default().fg(th().accent2)),
        ]))
        .style(Style::default().bg(theme::panel2())),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(rows[1].width as usize),
            Style::default().fg(th().border),
        ))),
        rows[1],
    );

    let ranked = app.filter_ranked(kind, query);
    let source = app.filter_source(kind);
    let (start, end) = window(ranked.len(), selected, rows[2].height as usize);
    let width = rows[2].width as usize;
    let now_id = app.now_playing.as_ref().map(|track| track.id.as_str());
    let mut lines = Vec::new();
    for position in start..end {
        let Some(index) = ranked.get(position) else {
            continue;
        };
        let Some(track) = source.get(*index) else {
            continue;
        };
        let active = position == selected;
        lines.push(track_line(
            track,
            active,
            now_id == Some(track.id.as_str()),
            app.is_favorite(&track.id),
            app.marked.contains_key(&track.id),
            width,
            None,
        ));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel())),
        rows[2],
    );
}

fn draw_playlist_picker(frame: &mut Frame, app: &App, selected: usize, area: Rect) {
    let height = (app.playlists.len() as u16 + 7).min(area.height.saturating_sub(4));
    let popup = centered_fixed(area, 58, height.max(9));
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().purple))
        .title(Span::styled(
            " add to playlist ",
            Style::default()
                .fg(th().purple)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let total = app.playlists.len() + 1;
    let (start, end) = window(total, selected, inner.height as usize);
    let width = inner.width as usize;
    let mut lines = Vec::new();
    for index in start..end {
        let active = index == selected;
        let style = if active {
            Style::default()
                .bg(th().purple)
                .fg(th().bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(th().fg)
        };
        let label = if index < app.playlists.len() {
            let playlist = &app.playlists[index];
            format!(" ♫ {}  ({} tracks)", playlist.name, playlist.tracks.len())
        } else {
            " + New playlist…".to_string()
        };
        lines.push(Line::from(Span::styled(truncate(&label, width), style)));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme::panel()))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn draw_new_playlist(frame: &mut Frame, name: &str, area: Rect) {
    let popup = centered_fixed(area, 58, 9);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().purple))
        .title(Span::styled(
            " new playlist ",
            Style::default()
                .fg(th().purple)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines = vec![
        Line::from(Span::styled(
            "playlist name",
            Style::default().fg(th().faint),
        )),
        Line::from(vec![
            Span::styled(" ❯ ", Style::default().fg(th().purple)),
            Span::styled(name.to_string(), Style::default().fg(th().fg)),
            Span::styled("▌", Style::default().fg(th().purple)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(th().accent2)),
            Span::styled(" create · ", Style::default().fg(th().dim)),
            Span::styled("Esc", Style::default().fg(th().accent2)),
            Span::styled(" cancel", Style::default().fg(th().dim)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel())),
        inner,
    );
}

fn draw_rename_playlist(frame: &mut Frame, name: &str, area: Rect) {
    let popup = centered_fixed(area, 58, 9);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().purple))
        .title(Span::styled(
            " rename playlist ",
            Style::default()
                .fg(th().purple)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines = vec![
        Line::from(Span::styled(
            "playlist name",
            Style::default().fg(th().faint),
        )),
        Line::from(vec![
            Span::styled(" ❯ ", Style::default().fg(th().purple)),
            Span::styled(name.to_string(), Style::default().fg(th().fg)),
            Span::styled("▌", Style::default().fg(th().purple)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(th().accent2)),
            Span::styled(" rename · ", Style::default().fg(th().dim)),
            Span::styled("Esc", Style::default().fg(th().accent2)),
            Span::styled(" cancel", Style::default().fg(th().dim)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel())),
        inner,
    );
}

fn draw_import_link(frame: &mut Frame, input: &str, area: Rect) {
    let popup = centered_fixed(area, 78, 11);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().accent2))
        .title(Span::styled(
            " import from link ",
            Style::default()
                .fg(th().accent2)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let width = inner.width.saturating_sub(4) as usize;
    let count = input.chars().count();
    let shown: String = if width > 0 && count > width {
        input.chars().skip(count - width).collect()
    } else {
        input.to_string()
    };
    let lines = vec![
        Line::from(Span::styled(
            " YouTube, SoundCloud, Bandcamp or a .m3u link.",
            Style::default().fg(th().fg),
        )),
        Line::from(Span::styled(
            " A single song starts playing; a playlist is saved and queued.",
            Style::default().fg(th().dim),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default().bg(theme::bg())),
            Span::styled(
                format!("{shown}▌"),
                Style::default()
                    .bg(theme::panel2())
                    .fg(th().fg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Enter", Style::default().fg(th().accent)),
            Span::styled(" import · ", Style::default().fg(th().dim)),
            Span::styled("Ctrl+V", Style::default().fg(th().accent)),
            Span::styled(" paste · ", Style::default().fg(th().dim)),
            Span::styled("Esc", Style::default().fg(th().accent)),
            Span::styled(" cancel", Style::default().fg(th().dim)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme::panel()))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn draw_sleep(frame: &mut Frame, app: &App, selected: usize, area: Rect) {
    let popup = centered_fixed(area, 50, 15);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().accent2))
        .title(Span::styled(
            " sleep timer ",
            Style::default()
                .fg(th().accent2)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines = Vec::new();
    for (index, (label, _)) in SLEEP_OPTIONS.iter().enumerate() {
        let active = index == selected;
        let style = if active {
            Style::default()
                .bg(th().accent2)
                .fg(th().bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(th().fg)
        };
        lines.push(Line::from(Span::styled(
            format!(" {} {}", if active { "▌" } else { " " }, label),
            style,
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        app.sleep_label()
            .map(|label| format!(" active: {label}"))
            .unwrap_or_else(|| " active: none".to_string()),
        Style::default().fg(th().dim),
    )));
    lines.push(Line::from(vec![
        Span::styled(" Enter", Style::default().fg(th().accent)),
        Span::styled(" set · ", Style::default().fg(th().dim)),
        Span::styled("Esc", Style::default().fg(th().accent)),
        Span::styled(" cancel", Style::default().fg(th().dim)),
    ]));
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel())),
        inner,
    );
}

fn draw_confirm(frame: &mut Frame, message: &str, area: Rect) {
    let popup = centered_fixed(area, 62, 9);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().red))
        .title(Span::styled(
            " confirm ",
            Style::default().fg(th().red).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let lines = vec![
        Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(th().fg),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("y / Enter", Style::default().fg(th().red)),
            Span::styled(" confirm · ", Style::default().fg(th().dim)),
            Span::styled("n / Esc", Style::default().fg(th().accent2)),
            Span::styled(" cancel", Style::default().fg(th().dim)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme::panel()))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn draw_lyrics(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered(area, 82, 86);
    frame.render_widget(Clear, popup);
    let Some(Overlay::Lyrics(lyrics)) = app.overlay.as_ref() else {
        return;
    };
    let source = lyrics
        .doc
        .as_ref()
        .map(|doc| doc.source.as_str())
        .unwrap_or("loading");
    let follow = if lyrics.follow {
        "follow on"
    } else {
        "follow off"
    };
    let title = format!(
        " ♫ lyrics · {} — {} · {} · {} · Esc close ",
        lyrics.track.title, lyrics.track.artist, source, follow
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th().purple))
        .title(Span::styled(
            truncate(&title, popup.width.saturating_sub(2) as usize),
            Style::default()
                .fg(th().purple)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::panel()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if lyrics.loading {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!(
                        "  {} fetching lyrics from the active lyrics plugin…",
                        app.spinner_char()
                    ),
                    Style::default().fg(th().yellow),
                )),
                Line::from(Span::styled(
                    "  synced lyrics scroll automatically while the track plays",
                    Style::default().fg(th().dim),
                )),
            ])
            .style(Style::default().bg(theme::panel())),
            inner,
        );
        return;
    }

    if let Some(error) = &lyrics.error {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  {error}"),
                    Style::default().fg(th().red),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Try another lyrics plugin, or press y again after changing providers.",
                    Style::default().fg(th().dim),
                )),
            ])
            .style(Style::default().bg(theme::panel()))
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }

    if lyrics.lines.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  The plugin returned no lyrics.",
                    Style::default().fg(th().dim),
                )),
            ])
            .style(Style::default().bg(theme::panel())),
            inner,
        );
        return;
    }

    let current = app.lyrics_current_line();
    let total = lyrics.lines.len();
    let height = inner.height as usize;
    let start = lyrics.scroll.min(total.saturating_sub(height));
    let end = (start + height).min(total);
    let mut lines = Vec::new();
    for index in start..end {
        let lyric = &lyrics.lines[index];
        let is_current = current == Some(index);
        let synced = lyric.at.is_some();
        let time = lyric
            .at
            .map(|at| format!("{:>5} ", format_position(at)))
            .unwrap_or_else(|| "      ".to_string());
        let style = if is_current {
            Style::default()
                .fg(th().bg)
                .bg(th().accent)
                .add_modifier(Modifier::BOLD)
        } else if synced {
            Style::default().fg(th().fg)
        } else {
            Style::default().fg(th().dim)
        };
        let prefix_style = if is_current {
            Style::default().bg(th().accent).fg(th().bg)
        } else {
            Style::default().fg(th().faint)
        };
        let text = format!(
            "{}{}",
            if is_current { " ▶ " } else { "   " },
            if lyric.text.is_empty() {
                "♪"
            } else {
                &lyric.text
            }
        );
        lines.push(Line::from(vec![
            Span::styled(time, prefix_style),
            Span::styled(text, style),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme::panel()))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
fn render_track_list(
    frame: &mut Frame,
    area: Rect,
    tracks: &[Track],
    selected: usize,
    now_id: Option<&str>,
    favorite: impl Fn(&Track) -> bool,
    marked: impl Fn(&Track) -> bool,
    show_number: bool,
) -> Vec<(usize, usize)> {
    let (start, end) = window(tracks.len(), selected, area.height as usize);
    let width = area.width as usize;
    let mut lines = Vec::new();
    let mut hits = Vec::new();
    for index in start..end {
        let track = &tracks[index];
        let playing = now_id == Some(track.id.as_str());
        hits.push((index, lines.len()));
        lines.push(track_line(
            track,
            index == selected,
            playing,
            favorite(track),
            marked(track),
            width,
            if show_number { Some(index + 1) } else { None },
        ));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::panel2())),
        area,
    );
    hits
}

fn track_line(
    track: &Track,
    selected: bool,
    playing: bool,
    favorite: bool,
    marked: bool,
    width: usize,
    number: Option<usize>,
) -> Line<'static> {
    let base = if selected {
        Style::default()
            .bg(th().accent)
            .fg(th().bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th().fg)
    };
    let dim = if selected {
        Style::default().bg(th().accent).fg(th().bg)
    } else {
        Style::default().fg(th().dim)
    };
    let marker = if playing {
        "▶"
    } else if marked {
        "✓"
    } else if favorite {
        "★"
    } else if selected {
        "▌"
    } else {
        " "
    };
    let marker_style = if selected {
        Style::default()
            .bg(th().accent)
            .fg(th().bg)
            .add_modifier(Modifier::BOLD)
    } else if playing {
        Style::default().fg(th().green).add_modifier(Modifier::BOLD)
    } else if marked {
        Style::default()
            .fg(th().accent2)
            .add_modifier(Modifier::BOLD)
    } else if favorite {
        Style::default().fg(th().yellow)
    } else {
        Style::default().fg(th().faint)
    };
    let duration = track.duration_label();
    let number_text = number
        .map(|value| format!("{value:>3} "))
        .unwrap_or_default();
    let left_prefix = format!("{}{} ", number_text, marker);
    let right = format!(" {} ", duration);
    let title_width = width.saturating_sub(display_width(&left_prefix) + display_width(&right) + 1);
    let label = if track.artist.trim().is_empty() {
        track.title.clone()
    } else {
        format!("{}  ·  {}", track.title, track.artist)
    };
    let label = truncate(&label, title_width);
    let used = display_width(&left_prefix) + display_width(&label) + display_width(&right);
    let pad = width.saturating_sub(used).max(1);
    Line::from(vec![
        Span::styled(left_prefix, marker_style),
        Span::styled(label, base),
        Span::styled(" ".repeat(pad), base),
        Span::styled(right, dim),
    ])
}

fn draw_empty(frame: &mut Frame, area: Rect, title: &str, hint: &str) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {title}"),
            Style::default().fg(th().fg).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  {hint}"),
            Style::default().fg(th().dim),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme::panel2()))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn window(total: usize, selected: usize, height: usize) -> (usize, usize) {
    if height == 0 || total == 0 {
        return (0, 0);
    }
    if total <= height {
        return (0, total);
    }
    let selected = selected.min(total - 1);
    let half = height / 2;
    let start = selected.saturating_sub(half).min(total - height);
    (start, start + height)
}

fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn centered_fixed(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}
