use crate::{
    app::{App, Focus, Mode, REACTIONS},
    model::{SOURCES, clean, one_line, source_label, time_label},
};
use qrcode::QrCode;
use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::collections::BTreeMap;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub panel: Color,
    pub raised: Color,
    pub line: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub purple: Color,
    pub orange: Color,
    pub red: Color,
    pub aqua: Color,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color::Rgb(29, 32, 33),
            panel: Color::Rgb(40, 40, 40),
            raised: Color::Rgb(60, 56, 54),
            line: Color::Rgb(80, 73, 69),
            text: Color::Rgb(235, 219, 178),
            muted: Color::Rgb(168, 153, 132),
            faint: Color::Rgb(124, 111, 100),
            green: Color::Rgb(184, 187, 38),
            yellow: Color::Rgb(250, 189, 47),
            blue: Color::Rgb(131, 165, 152),
            purple: Color::Rgb(211, 134, 155),
            orange: Color::Rgb(254, 128, 25),
            red: Color::Rgb(251, 73, 52),
            aqua: Color::Rgb(142, 192, 124),
        }
    }
}
impl Theme {
    pub fn source(self, id: &str) -> Color {
        match id {
            "slack" => self.purple,
            "discord" => self.blue,
            "whatsapp" => self.green,
            "iphone" => self.aqua,
            "mail:home" => self.orange,
            "mail:cmu" => self.yellow,
            _ => self.muted,
        }
    }
}
fn panel<'a>(title: impl Into<Line<'a>>, focus: bool, t: Theme) -> Block<'a> {
    Block::new()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if focus { t.green } else { t.line }))
        .style(Style::new().bg(t.panel))
}
pub fn render(frame: &mut Frame, app: &App) {
    let t = Theme::default();
    let area = frame.area();
    frame.render_widget(Block::new().bg(t.bg).fg(t.text), area);
    if area.width < 52 || area.height < 16 {
        frame.render_widget(
            Paragraph::new("Chord\n\nMake the terminal at least 52 × 16.\nq to quit")
                .style(Style::new().fg(t.text)),
            area,
        );
        return;
    }
    let outer = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(7),
        Constraint::Length(2),
    ])
    .split(area.inner(Margin {
        horizontal: 1,
        vertical: 0,
    }));
    let header = Layout::horizontal([Constraint::Min(20), Constraint::Length(44)]).split(outer[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  c", Style::new().fg(t.text).bold()),
            Span::styled("·", Style::new().fg(t.green).bold()),
            Span::styled("  chord", Style::new().fg(t.text).bold()),
            Span::styled("   /   conversations", Style::new().fg(t.muted)),
        ]))
        .block(Block::new().padding(ratatui::widgets::Padding::vertical(1))),
        header[0],
    );
    let health = if app.demo {
        "design preview"
    } else {
        "local · independent"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("● ", Style::new().fg(t.green)),
            Span::styled(health, Style::new().fg(t.muted)),
            Span::styled(
                format!("   {}", chrono::Local::now().format("%a %H:%M")),
                Style::new().fg(t.faint),
            ),
        ]))
        .alignment(Alignment::Right)
        .block(Block::new().padding(ratatui::widgets::Padding::vertical(1))),
        header[1],
    );
    let n = app.visible().len();
    let tabs = vec![
        Span::styled(
            format!(
                "  {}  ",
                if app.unread {
                    "UNREAD"
                } else if app.focus_enabled() {
                    "FOCUS"
                } else {
                    "ALL"
                }
            ),
            Style::new().fg(t.green).bold(),
        ),
        Span::styled(format!("{n} conversations"), Style::new().fg(t.muted)),
        Span::styled("   / ", Style::new().fg(t.faint)),
        Span::styled(app.source_title(), Style::new().fg(t.text)),
        Span::styled(
            format!(
                "   {} quieter · v {}",
                app.inbox_summary["hidden"].as_u64().unwrap_or(0),
                if app.focus_enabled() {
                    "show all"
                } else {
                    "focus"
                }
            ),
            Style::new().fg(t.faint),
        ),
        Span::styled(
            if app.loading { "   syncing…" } else { "" },
            Style::new().fg(t.yellow),
        ),
    ];
    frame.render_widget(Paragraph::new(Line::from(tabs)), outer[1]);
    let (sidebar, list, content) = if area.width >= 125 {
        let a = Layout::horizontal([
            Constraint::Length(24),
            Constraint::Length(39),
            Constraint::Min(45),
        ])
        .spacing(1)
        .split(outer[2]);
        (Some(a[0]), a[1], a[2])
    } else {
        let a = Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)])
            .spacing(1)
            .split(outer[2]);
        (None, a[0], a[1])
    };
    if let Some(side) = sidebar {
        render_sources(frame, app, side, t)
    }
    render_inbox(frame, app, list, t);
    render_thread(frame, app, content, t);
    let hint = match app.mode {
        Mode::Compose => "esc back    ctrl+s send    enter newline",
        Mode::Search => "enter search    esc clear",
        Mode::Accounts => "j/k select    enter connect    esc back",
        Mode::Login => "tab next field    ctrl+s submit    esc back",
        Mode::React => "h/j/k/l choose    enter react    esc back",
        Mode::Confirm => "enter send    esc cancel",
        Mode::Attach => "enter attach    esc cancel",
        Mode::Image => "←/→ images    d save original    b open in Brave    R retry    esc back",
        Mode::Servers => "type to find a server    ↑/↓ choose    enter show channels    esc back",
        Mode::Contacts | Mode::ContactLink => {
            "type to search    ↑/↓ select    enter open    esc back"
        }
        Mode::Details => {
            "e rename    * favorite    m link identity    u undo link    enter open chat    esc back"
        }
        Mode::ContactName => "enter save name    esc cancel",
        Mode::ConfirmContactLink => "enter link these accounts    esc cancel",
        _ => {
            "j/k move   i write   r reply   e react   v focus/all   c contacts   * pin   z quiet   ? help"
        }
    };
    let status = Layout::horizontal([Constraint::Min(15), Constraint::Length(19)]).split(outer[3]);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {}", one_line(&app.notice)),
                Style::new().fg(if app.notice.to_lowercase().contains("failed") {
                    t.red
                } else {
                    t.muted
                }),
            )),
            Line::from(Span::styled(format!(" {hint}"), Style::new().fg(t.faint))),
        ]),
        status[0],
    );
    frame.render_widget(
        Paragraph::new("GRUVBOX DARK  ●")
            .style(Style::new().fg(t.faint))
            .alignment(Alignment::Right),
        status[1],
    );
    match app.mode {
        Mode::Image => render_image_viewer(frame, app, t),
        Mode::Search => render_search(frame, app, t),
        Mode::React => render_reactions(frame, app, t),
        Mode::Confirm => render_confirmation(frame, app, t),
        Mode::Help => render_help(frame, t),
        Mode::Accounts => render_accounts(frame, app, t),
        Mode::Login => render_login(frame, app, t),
        Mode::Attach => render_attach(frame, app, t),
        Mode::Servers => render_servers(frame, app, t),
        Mode::Contacts
        | Mode::ContactLink
        | Mode::Details
        | Mode::ContactName
        | Mode::ConfirmContactLink => render_contacts(frame, app, t),
        _ => {}
    }
}
fn render_sources(frame: &mut Frame, app: &App, area: Rect, t: Theme) {
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    frame.render_widget(Block::new().style(Style::new().bg(t.bg)), area);
    let mut rows = vec![
        Line::from(Span::styled("WORKSPACE", Style::new().fg(t.faint))),
        Line::from(""),
    ];
    for (i, (_id, label, key)) in SOURCES.iter().enumerate() {
        let active = app.source == i;
        rows.push(Line::from(vec![
            Span::styled(if active { "▎ " } else { "  " }, Style::new().fg(t.green)),
            Span::styled(format!("{key} "), Style::new().fg(t.faint)),
            Span::styled(
                (*label).to_owned(),
                Style::new()
                    .fg(if active { t.text } else { t.muted })
                    .add_modifier(if active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ]));
        if i == 2 {
            rows.push(Line::from(Span::styled(
                "    s Servers…",
                Style::new().fg(if active { t.blue } else { t.faint }),
            )));
            if active && app.discord_scope.is_some() {
                rows.push(Line::from(Span::styled(
                    format!("    {}", one_line(&app.server_label)),
                    Style::new().fg(t.blue),
                )));
            }
        }
        rows.push(Line::from(if i == 0 { "" } else { " " }));
        if i == 4 {
            rows.push(Line::from(Span::styled("  MAIL", Style::new().fg(t.faint))));
        }
    }
    frame.render_widget(Paragraph::new(rows), inner);
    if area.height > 27 {
        let bottom = Rect {
            x: inner.x + 1,
            y: area.bottom() - 8,
            width: inner.width.saturating_sub(1),
            height: 6,
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("ON THIS LAPTOP", Style::new().fg(t.faint))),
                Line::from(""),
                Line::from(vec![
                    Span::styled("● ", Style::new().fg(t.green)),
                    Span::styled("Matrix + bridges", Style::new().fg(t.muted)),
                ]),
                Line::from(vec![
                    Span::styled("● ", Style::new().fg(t.aqua)),
                    Span::styled("BlueFerry", Style::new().fg(t.muted)),
                ]),
                Line::from(vec![
                    Span::styled("● ", Style::new().fg(t.orange)),
                    Span::styled("Google mail", Style::new().fg(t.muted)),
                ]),
            ]),
            bottom,
        );
    }
}
fn render_inbox(frame: &mut Frame, app: &App, area: Rect, t: Theme) {
    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            if app.unread {
                "Unread"
            } else if app.focus_enabled() {
                "Focus"
            } else if app.discord_scope.is_some() {
                "Channels"
            } else {
                "All conversations"
            },
            Style::new().fg(t.text).bold(),
        ),
        Span::styled(
            format!("  {} ", app.visible().len()),
            Style::new().fg(t.faint),
        ),
    ]);
    let block = panel(title, app.focus == Focus::Inbox, t);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let visible = app.visible();
    if visible.is_empty() {
        let text = if app.loading {
            "\n  Loading your conversations…"
        } else if app.focus_enabled() {
            "\n  Nothing needs your attention here.\n\n  v shows all conversations.\n  * pins a chat in Focus.\n  c opens your contacts."
        } else {
            "\n  Nothing here yet.\n\n  Press a to connect your accounts.\n  Email and iPhone appear as\n  soon as they are available."
        };
        frame.render_widget(
            Paragraph::new(text)
                .style(Style::new().fg(t.muted))
                .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let selected = i == app.selection;
            let bg = if selected { t.raised } else { t.panel };
            let width = inner.width as usize;
            let reason = c.importance["reasons"][0].as_str().unwrap_or("");
            let reason = match reason {
                "Recent direct conversation" => "Direct message",
                "Unread direct message" => "Unread DM",
                "You participate in this conversation" => "Active chat",
                "Reply to your message" => "Reply to you",
                "Marked important in email" => "Important email",
                "Unread personal email" => "Personal email",
                "Pinned by you" => "Pinned",
                "Favorite contact" => "Favorite",
                _ => reason,
            };
            let label = if app.focus_enabled() && !reason.is_empty() {
                format!(
                    "{} · {}",
                    c.server_name.as_deref().unwrap_or(source_label(&c.source)),
                    reason
                )
            } else {
                c.server_name
                    .as_deref()
                    .unwrap_or(source_label(&c.source))
                    .to_owned()
            };
            let first = Line::from(vec![
                Span::styled(if c.unread { "● " } else { "  " }, Style::new().fg(t.green)),
                Span::styled(
                    one_line(&c.title),
                    Style::new().fg(t.text).add_modifier(if c.unread {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
            ]);
            let second = Line::from(Span::styled(
                format!(
                    "  {}",
                    one_line(&c.preview)
                        .chars()
                        .take(width.saturating_sub(4))
                        .collect::<String>()
                ),
                Style::new().fg(t.muted),
            ));
            let time = time_label(&c.timestamp);
            let label: String = label
                .chars()
                .take(width.saturating_sub(time.len() + 4))
                .collect();
            let gap = width.saturating_sub(UnicodeWidthStr::width(label.as_str()) + time.len() + 4);
            let third = Line::from(vec![
                Span::raw("  "),
                Span::styled(label, Style::new().fg(t.source(&c.source))),
                Span::raw(" ".repeat(gap)),
                Span::styled(time, Style::new().fg(t.faint)),
            ]);
            ListItem::new(vec![first, second, third, Line::from("")]).style(Style::new().bg(bg))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.selection));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::new().bg(t.raised))
            .highlight_symbol(""),
        inner,
        &mut state,
    );
}
fn render_thread(frame: &mut Frame, app: &App, area: Rect, t: Theme) {
    let Some(c) = app.active.as_ref() else {
        frame.render_widget(panel("", false, t), area);
        let inner = area.inner(Margin {
            horizontal: 3,
            vertical: 3,
        });
        let welcome = vec![
            Line::from(""),
            Line::from(Span::styled("c·", Style::new().fg(t.green).bold())),
            Line::from(""),
            Line::from(Span::styled(
                "A little less switching.",
                Style::new().fg(t.text).bold(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Chats and email, in one quiet place.",
                Style::new().fg(t.muted),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Choose a conversation and press enter.",
                Style::new().fg(t.faint),
            )),
        ];
        frame.render_widget(Paragraph::new(welcome).alignment(Alignment::Center), inner);
        return;
    };
    let writing = app.mode == Mode::Compose || !app.editor.text.is_empty();
    let height = if writing {
        (app.editor.text.lines().count() as u16 + 4).clamp(6, 12)
    } else {
        4
    };
    let layout = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(3),
        Constraint::Length(height),
    ])
    .spacing(1)
    .split(area);
    let subtitle = format!(
        "{}  ·  {}",
        source_label(&c.source),
        if c.kind == "email" {
            "email thread"
        } else if c.kind == "phone" {
            "through your iPhone"
        } else {
            c.server_name.as_deref().unwrap_or("conversation")
        }
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {}", clean(&c.title)),
                Style::new().fg(t.text).bold(),
            )),
            Line::from(Span::styled(
                format!(" {}", subtitle),
                Style::new().fg(t.source(&c.source)),
            )),
        ])
        .block(
            Block::new()
                .borders(Borders::BOTTOM)
                .border_style(Style::new().fg(t.line)),
        ),
        layout[0],
    );
    let mut message_area = layout[1];
    if app.mode != Mode::Image
        && app.focus == Focus::Messages
        && app.image_target().is_some()
        && message_area.height >= 12
    {
        let parts = Layout::vertical([
            Constraint::Min(5),
            Constraint::Length((message_area.height / 2).clamp(6, 14)),
        ])
        .spacing(1)
        .split(message_area);
        message_area = parts[0];
        render_image(frame, app, parts[1], t);
    }
    let mut lines: Vec<Line<'static>> = vec![];
    let mut ranges = vec![];
    let mut day = String::new();
    let width = message_area.width.saturating_sub(5) as usize;
    for (i, m) in app.messages.iter().enumerate() {
        let stamp = chrono::DateTime::parse_from_rfc3339(&m.timestamp).ok();
        let date = stamp
            .map(|d| d.format("%A, %B %d").to_string())
            .unwrap_or_default();
        if date != day {
            day = date.clone();
            lines.push(Line::from(Span::styled(
                format!("  {date}"),
                Style::new().fg(t.faint),
            )));
            lines.push(Line::from(""));
        }
        let start = lines.len();
        let selected = app.focus == Focus::Messages && i == app.message_selection;
        let name = if m.is_sender {
            "You".into()
        } else {
            one_line(&m.sender_name)
        };
        let bg = if selected { t.raised } else { t.panel };
        let style = Style::new().bg(bg);
        lines.push(
            Line::from(vec![
                Span::styled(
                    if selected { " ▎ " } else { "   " },
                    Style::new().fg(t.green),
                ),
                Span::styled(
                    name,
                    Style::new()
                        .fg(if m.is_sender {
                            t.green
                        } else {
                            t.source(&c.source)
                        })
                        .bold(),
                ),
                Span::styled(
                    format!("   {}", time_label(&m.timestamp)),
                    Style::new().fg(t.faint),
                ),
            ])
            .style(style),
        );
        if m.linked_message_id.is_some() {
            let original = app
                .messages
                .iter()
                .find(|r| Some(&r.id) == m.linked_message_id.as_ref());
            let preview = original
                .map(|r| one_line(&r.text))
                .unwrap_or_else(|| "An earlier message".into());
            for l in wrap_text(&format!("↳ {preview}"), width) {
                lines.push(
                    Line::from(Span::styled(format!("   {l}"), Style::new().fg(t.muted)))
                        .style(style),
                );
            }
        }
        for l in wrap_text(&clean(&m.text), width) {
            lines.push(
                Line::from(Span::styled(format!("   {l}"), Style::new().fg(t.text))).style(style),
            );
        }
        for a in &m.attachments {
            lines.push(
                Line::from(Span::styled(
                    format!(
                        "   {} {}{}",
                        if crate::images::is_image(a) {
                            "▧"
                        } else {
                            "↓"
                        },
                        a["fileName"].as_str().unwrap_or("Attachment"),
                        if crate::images::is_image(a) {
                            " · o view image"
                        } else {
                            ""
                        }
                    ),
                    Style::new().fg(t.aqua),
                ))
                .style(style),
            );
        }
        let mut reactions = BTreeMap::<String, usize>::new();
        for r in &m.reactions {
            *reactions
                .entry(r["reactionKey"].as_str().unwrap_or("?").to_owned())
                .or_default() += 1
        }
        if !reactions.is_empty() {
            lines.push(
                Line::from(Span::styled(
                    format!(
                        "   {}",
                        reactions
                            .iter()
                            .map(|(k, v)| format!("{k} {v}"))
                            .collect::<Vec<_>>()
                            .join("   ")
                    ),
                    Style::new().fg(t.yellow),
                ))
                .style(style),
            );
        }
        ranges.push((start, lines.len()));
        lines.push(Line::from(""));
        lines.push(Line::from(""));
    }
    if app.message_loading && lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Loading conversation…",
            Style::new().fg(t.muted),
        )));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No messages loaded yet.",
            Style::new().fg(t.muted),
        )));
    }
    let total = lines.len();
    let h = message_area.height as usize;
    let offset = if let Some((start, end)) = ranges.get(app.message_selection) {
        if *end >= total.saturating_sub(3) {
            total.saturating_sub(h)
        } else {
            start.saturating_sub(2).min(total.saturating_sub(h))
        }
    } else {
        0
    };
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::new().bg(t.panel))
            .scroll((offset.min(u16::MAX as usize) as u16, 0)),
        message_area,
    );
    let title = if let Some(reply) = &app.reply {
        format!(" Reply to {} ", one_line(&reply.sender_name))
    } else if app.edit_message.is_some() {
        " Edit message ".into()
    } else if writing {
        " Write a message ".into()
    } else {
        " i to write · r to reply ".into()
    };
    let block = panel(title, app.mode == Mode::Compose, t);
    let inner = block.inner(layout[2]);
    frame.render_widget(block, layout[2]);
    if !c.can_send {
        frame.render_widget(
            Paragraph::new("Connect this account to send.").style(Style::new().fg(t.faint)),
            inner,
        );
    } else {
        let text = if app.editor.text.is_empty() {
            if c.kind == "email" {
                "Email replies stay in memory until you explicitly send."
            } else {
                "Write something…"
            }
        } else {
            &app.editor.text
        };
        frame.render_widget(
            Paragraph::new(clean(text))
                .style(Style::new().fg(if app.editor.text.is_empty() {
                    t.faint
                } else {
                    t.text
                }))
                .wrap(Wrap { trim: false }),
            inner,
        );
        if app.mode == Mode::Compose {
            let before = &app.editor.text[..app.editor.cursor];
            let y = before.split('\n').count().saturating_sub(1) as u16;
            let x = UnicodeWidthStr::width(before.rsplit('\n').next().unwrap_or("")) as u16;
            frame.set_cursor_position((
                inner.x + x.min(inner.width.saturating_sub(1)),
                inner.y + y.min(inner.height.saturating_sub(1)),
            ));
        }
    }
    if let Some(a) = &app.pending_attachment {
        let r = Rect {
            x: inner.x,
            y: layout[2].bottom().saturating_sub(2),
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(format!(
                "↓ {} · ctrl+s to send",
                a["fileName"].as_str().unwrap_or("attachment")
            ))
            .style(Style::new().fg(t.aqua)),
            r,
        );
    }
}
fn render_image_viewer(frame: &mut Frame, app: &App, t: Theme) {
    let area = frame.area();
    let r = modal(
        frame,
        area.width.saturating_sub(2),
        area.height.saturating_sub(4),
        "Image gallery",
        t,
    );
    let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
        .spacing(1)
        .split(r);
    render_image(frame, app, parts[0], t);
    frame.render_widget(
        Paragraph::new("←/→ browse · d save · b Brave · R retry · esc back")
            .style(Style::new().fg(t.yellow)),
        parts[1],
    );
}
fn render_image(frame: &mut Frame, app: &App, area: Rect, t: Theme) {
    let Some(target) = app.image_target() else {
        return;
    };
    let key = target.key();
    let count = app.image_targets().len();
    let title = format!(
        " ▧ {} · {}/{} · o enlarge ",
        one_line(target.name()),
        app.images.index.min(count.saturating_sub(1)) + 1,
        count
    );
    let block = panel(title, false, t);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    match app.images.cache.get(&key) {
        Some(Ok(loaded)) => {
            let size = loaded.data.dimensions;
            let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
            let resize = ratatui_image::Resize::Fit(Some(image::imageops::FilterType::Triangle));
            let mut protocol = loaded.protocol.borrow_mut();
            let fitted = protocol.size_for(resize.clone(), rows[0].as_size());
            let centered = Rect::new(
                rows[0].x + rows[0].width.saturating_sub(fitted.width) / 2,
                rows[0].y + rows[0].height.saturating_sub(fitted.height) / 2,
                fitted.width.min(rows[0].width),
                fitted.height.min(rows[0].height),
            );
            frame.render_stateful_widget(
                ratatui_image::StatefulImage::default().resize(resize),
                centered,
                &mut *protocol,
            );
            frame.render_widget(
                Paragraph::new(format!(
                    "{} × {} · {} KB",
                    size.0,
                    size.1,
                    loaded.data.bytes.len().div_ceil(1024)
                ))
                .alignment(Alignment::Center)
                .style(Style::new().fg(t.muted)),
                rows[1],
            );
        }
        Some(Err(error)) => {
            frame.render_widget(
                Paragraph::new(format!(
                    "{}\nR in the gallery retries this image.",
                    clean(error)
                ))
                .style(Style::new().fg(t.red))
                .wrap(Wrap { trim: false }),
                inner,
            );
        }
        None => {
            frame.render_widget(
                Paragraph::new("Loading image…")
                    .style(Style::new().fg(t.muted))
                    .alignment(Alignment::Center),
                inner,
            );
        }
    }
}
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = vec![];
    for line in text.lines() {
        if line.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut used = 0;
        for word in line.split_whitespace() {
            let w = UnicodeWidthStr::width(word);
            if used > 0 && used + 1 + w > width {
                lines.push(std::mem::take(&mut current));
                used = 0
            }
            if used > 0 {
                current.push(' ');
                used += 1
            }
            for ch in word.chars() {
                let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if used + cw > width && !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                    used = 0
                }
                current.push(ch);
                used += cw;
            }
        }
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new())
    }
    lines
}
fn modal(frame: &mut Frame, width: u16, height: u16, title: &str, t: Theme) -> Rect {
    let area = frame.area();
    let w = width.min(area.width.saturating_sub(4));
    let h = height.min(area.height.saturating_sub(4));
    let r = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    frame.render_widget(Clear, r);
    let b = panel(format!(" {title} "), true, t);
    let inner = b.inner(r).inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    frame.render_widget(b, r);
    inner
}
fn render_search(frame: &mut Frame, app: &App, t: Theme) {
    let r = modal(frame, 80, 8, "Search", t);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Chat names, message text, or Gmail search syntax",
                Style::new().fg(t.muted),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("/ {}", clean(&app.search)),
                Style::new().fg(t.text),
            )),
        ]),
        r,
    );
    frame.set_cursor_position((
        r.x + 2 + UnicodeWidthStr::width(app.search.as_str()) as u16,
        r.y + 2,
    ));
}
fn render_servers(frame: &mut Frame, app: &App, t: Theme) {
    let area = modal(frame, 92, 32, "Discord servers", t);
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!("Search  {}", clean(&app.server_query)),
                Style::new().fg(t.text),
            )),
            Line::from(Span::styled(
                format!(
                    "{} servers · choosing a server shows all its channels",
                    app.servers.len()
                ),
                Style::new().fg(t.muted),
            )),
        ]),
        layout[0],
    );
    let entries: Vec<ListItem> = app
        .visible_servers()
        .iter()
        .map(|server| {
            let description = server["description"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    format!("{} channels", server["channelCount"].as_u64().unwrap_or(0))
                });
            ListItem::new(vec![
                Line::from(Span::styled(
                    format!(
                        "  {}",
                        one_line(server["name"].as_str().unwrap_or("Server"))
                    ),
                    Style::new().fg(t.text).bold(),
                )),
                Line::from(Span::styled(
                    format!("  {description}"),
                    Style::new().fg(t.faint),
                )),
            ])
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.server_selection));
    frame.render_stateful_widget(
        List::new(entries)
            .highlight_style(Style::new().bg(t.raised))
            .highlight_symbol("▎ "),
        layout[1],
        &mut state,
    );
    frame.render_widget(
        Paragraph::new("Type to search · ↑/↓ select · enter show channels · esc back").fg(t.blue),
        layout[2],
    );
}
fn render_contacts(frame: &mut Frame, app: &App, t: Theme) {
    match app.mode {
        Mode::ContactName => {
            let area = modal(frame, 80, 10, "Contact name", t);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        "A name to use across this person's linked accounts",
                        Style::new().fg(t.muted),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        format!("  {}", clean(&app.contact_editor.text)),
                        Style::new().fg(t.text).bold(),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "enter save    esc cancel",
                        Style::new().fg(t.green),
                    )),
                ])
                .wrap(Wrap { trim: false }),
                area,
            );
        }
        Mode::ConfirmContactLink => {
            let area = modal(frame, 94, 23, "Link these accounts to one person?", t);
            let mut lines = vec![];
            for person in [&app.link_source, &app.details] {
                lines.push(Line::from(Span::styled(
                    clean(person["name"].as_str().unwrap_or("Unknown contact")),
                    Style::new().fg(t.yellow).bold(),
                )));
                for identity in person["identities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(5)
                {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", clean(identity["key"].as_str().unwrap_or(""))),
                        Style::new().fg(t.muted),
                    )));
                }
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                "Link only if these accounts belong to the same person.",
                Style::new().fg(t.text),
            )));
            lines.push(Line::from(Span::styled(
                "enter link    esc cancel    u can undo the last link",
                Style::new().fg(t.green),
            )));
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        }
        Mode::Details => {
            let area = modal(frame, 104, 32, "Contact", t);
            let person = &app.details;
            let mut lines = vec![
                Line::from(Span::styled(
                    format!(
                        "{}{}",
                        if person["favorite"] == true {
                            "★ "
                        } else {
                            ""
                        },
                        clean(person["name"].as_str().unwrap_or("Unknown contact"))
                    ),
                    Style::new().fg(t.text).bold(),
                )),
                Line::from(Span::styled(
                    "e rename    * favorite    m link another account    u undo link",
                    Style::new().fg(t.green),
                )),
                Line::from(""),
                Line::from(Span::styled("ACCOUNTS", Style::new().fg(t.faint))),
            ];
            for identity in person["identities"]
                .as_array()
                .into_iter()
                .flatten()
                .take(9)
            {
                let source = identity["source"].as_str().unwrap_or("account");
                lines.push(Line::from(vec![
                    Span::styled(format!(" {source:<10}"), Style::new().fg(t.source(source))),
                    Span::styled(
                        clean(identity["key"].as_str().unwrap_or("")),
                        Style::new().fg(t.muted),
                    ),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "CONVERSATIONS   ↑/↓ choose · enter open",
                Style::new().fg(t.faint),
            )));
            if let Some(conversations) = person["conversations"].as_array() {
                for (i, c) in conversations.iter().enumerate().take(9) {
                    let chosen = i == app.contact_conversation;
                    lines.push(Line::from(Span::styled(
                        format!(
                            "{}{} · {}",
                            if chosen { "▎ " } else { "  " },
                            clean(c["title"].as_str().unwrap_or("Conversation")),
                            clean(c["source"].as_str().unwrap_or(""))
                        ),
                        Style::new()
                            .fg(if chosen { t.text } else { t.muted })
                            .bg(if chosen { t.raised } else { t.panel }),
                    )));
                }
                if conversations.is_empty() {
                    lines.push(Line::from("No loaded conversation with this contact yet."));
                }
            }
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        }
        _ => {
            let title = if app.mode == Mode::ContactLink {
                "Link another account"
            } else {
                "Contacts"
            };
            let area = modal(frame, 104, 32, title, t);
            let layout = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(1),
            ])
            .split(area);
            let people = app.visible_contacts();
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!("Search  {}", clean(&app.contact_query)),
                        Style::new().fg(t.text),
                    )),
                    Line::from(Span::styled(
                        format!(
                            "{} contacts · names and accounts stay on this laptop",
                            people.len()
                        ),
                        Style::new().fg(t.muted),
                    )),
                ]),
                layout[0],
            );
            let entries: Vec<ListItem> = people
                .iter()
                .map(|p| {
                    let sources = p["identities"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|i| {
                            format!(
                                "{}{}{}",
                                i["source"].as_str().unwrap_or("account"),
                                i["label"]
                                    .as_str()
                                    .map(|s| format!(" / {s}"))
                                    .unwrap_or_default(),
                                i["handle"]
                                    .as_str()
                                    .map(|s| format!(" · @{s}"))
                                    .unwrap_or_default()
                            )
                        })
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join(" · ");
                    ListItem::new(vec![
                        Line::from(Span::styled(
                            format!(
                                "{}{}",
                                if p["favorite"] == true { "★ " } else { "  " },
                                clean(p["name"].as_str().unwrap_or("Unknown contact"))
                            ),
                            Style::new().fg(t.text),
                        )),
                        Line::from(Span::styled(
                            format!(
                                "  {sources}{}",
                                if p["unresolved"] == true {
                                    " · name needed"
                                } else {
                                    ""
                                }
                            ),
                            Style::new().fg(t.faint),
                        )),
                        Line::from(""),
                    ])
                })
                .collect();
            let mut state = ListState::default().with_selected(Some(app.contact_selection));
            frame.render_stateful_widget(
                List::new(entries)
                    .highlight_style(Style::new().bg(t.raised))
                    .highlight_symbol("▎ "),
                layout[1],
                &mut state,
            );
            frame.render_widget(
                Paragraph::new("Type to search · ↑/↓ select · enter open · esc back").fg(t.green),
                layout[2],
            );
        }
    }
}
fn render_reactions(frame: &mut Frame, app: &App, t: Theme) {
    let r = modal(frame, 56, 10, "React", t);
    let lines = REACTIONS
        .chunks(6)
        .enumerate()
        .map(|(row, items)| {
            Line::from(
                items
                    .iter()
                    .enumerate()
                    .map(|(col, e)| {
                        Span::styled(
                            format!("  {e}  "),
                            Style::new()
                                .fg(t.text)
                                .bg(if row * 6 + col == app.reaction {
                                    t.line
                                } else {
                                    t.panel
                                }),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), r);
}
fn render_confirmation(frame: &mut Frame, app: &App, t: Theme) {
    let r = modal(frame, 80, 17, "Send this message?", t);
    let body = app.confirm_body.as_ref();
    let target = body
        .and_then(|v| v["to"].as_str())
        .map(str::to_owned)
        .or_else(|| app.active.as_ref().map(|c| c.title.clone()))
        .unwrap_or_default();
    let mut lines = vec![
        Line::from(vec![
            Span::styled("To  ", Style::new().fg(t.faint)),
            Span::styled(clean(&target), Style::new().fg(t.yellow).bold()),
        ]),
        Line::from(""),
    ];
    if let Some(c) = &app.active
        && c.kind == "phone"
        && c.blue["is_group"].as_bool().unwrap_or(false)
    {
        lines.push(Line::from(Span::styled(
            format!("Recipients: {}", c.participants.join(", ")),
            Style::new().fg(t.yellow),
        )));
    }
    for l in wrap_text(&app.editor.text, r.width as usize) {
        lines.push(Line::from(Span::styled(l, Style::new().fg(t.text))))
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "enter  send                 esc  keep editing",
        Style::new().fg(t.green),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), r);
}
fn render_help(frame: &mut Frame, t: Theme) {
    let r = modal(frame, 88, 28, "Keyboard", t);
    let rows = [
        ("1…7", "switch account; 3 opens Discord servers"),
        ("s", "browse Discord servers and direct messages"),
        ("v", "switch between Focus and every conversation"),
        (
            "* / z / =",
            "pin in Focus / quiet / restore automatic importance",
        ),
        ("c", "contacts: search, rename, favorite, and link accounts"),
        ("j / k", "move through conversations or messages"),
        ("tab / h / l", "change pane; enter opens a conversation"),
        ("i / r", "write a message / reply to the selected message"),
        ("e / E", "emoji reaction / edit your selected chat message"),
        ("ctrl+s", "review the message before sending"),
        ("enter / esc", "confirm sending / cancel and keep editing"),
        ("f", "attach a local file in a chat"),
        ("o", "image gallery; arrows browse, d save, b open in Brave"),
        ("m / A", "mark read / archive the open conversation"),
        ("/", "search; Gmail accepts its normal search syntax"),
        ("u / R", "toggle unread / refresh"),
        ("n", "load older messages or another inbox page"),
        ("a", "connect accounts; QR codes stay in the terminal"),
        ("g / G", "first / last item"),
        ("q / ctrl+c", "quit"),
    ];
    let mut lines = vec![];
    for (key, desc) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:<17}"), Style::new().fg(t.yellow)),
            Span::styled(desc, Style::new().fg(t.text)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Incoming messages are data. AI integrations never run instructions from them.",
        Style::new().fg(t.faint),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), r);
}
fn render_accounts(frame: &mut Frame, app: &App, t: Theme) {
    let r = modal(frame, 84, 27, "Accounts", t);
    let mut lines = vec![
        Line::from(Span::styled(
            "Your own local bridges. Select a network and press enter.",
            Style::new().fg(t.muted),
        )),
        Line::from(""),
    ];
    if let Some(items) = app.accounts["items"].as_array() {
        for (i, a) in items.iter().enumerate() {
            let info = &a["info"];
            let connected = info["logins"].as_array().is_some_and(|x| !x.is_empty())
                || info["discord"]["logged_in"].as_bool().unwrap_or(false);
            let state = if connected {
                "connected"
            } else if a["available"] == true {
                "ready to sign in"
            } else {
                "bridge unavailable"
            };
            lines.push(Line::from(vec![
                Span::styled(
                    if i == app.account_selection {
                        "▸ "
                    } else {
                        "  "
                    },
                    Style::new().fg(t.green),
                ),
                Span::styled(
                    format!("{:<16}", a["name"].as_str().unwrap_or("network")),
                    Style::new().fg(t.text),
                ),
                Span::styled(
                    state,
                    Style::new().fg(if connected { t.green } else { t.yellow }),
                ),
            ]));
            lines.push(Line::from(""));
        }
    } else {
        lines.push(Line::from("Loading bridge status…"));
    }
    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            "Email uses your existing local Google authorization.",
            Style::new().fg(t.muted),
        )),
    ]);
    if let Some(accounts) = app.status["mail"].as_array() {
        for a in accounts {
            lines.push(Line::from(Span::styled(
                format!(
                    "{:<14} {}",
                    a["name"].as_str().unwrap_or("Mail"),
                    a["email"].as_str().unwrap_or("")
                ),
                Style::new().fg(t.orange),
            )))
        }
    }
    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            "iPhone     direct BlueFerry over Bluetooth",
            Style::new().fg(t.aqua),
        )),
    ]);
    for error in app.errors.iter().take(3) {
        lines.push(Line::from(Span::styled(
            clean(error),
            Style::new().fg(t.red),
        )))
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), r);
}
fn render_login(frame: &mut Frame, app: &App, t: Theme) {
    let qr = app.login["display_and_wait"]["data"].as_str();
    let r = modal(frame, 90, 46, &format!("Connect {}", app.login_id), t);
    let mut lines = wrap_text(
        app.login["instructions"].as_str().unwrap_or("Connecting…"),
        r.width as usize,
    )
    .into_iter()
    .map(|s| Line::from(Span::styled(clean(&s), Style::new().fg(t.text))))
    .collect::<Vec<_>>();
    lines.push(Line::from(""));
    if app.login["display_and_wait"]["type"] == "qr" {
        if let Some(data) = qr
            && let Ok(code) = QrCode::new(data)
        {
            let colors = code.to_colors();
            let w = code.width();
            let mut qrlines = vec![];
            for y in (0..w + 4).step_by(2) {
                let mut row = String::new();
                for x in 0..w + 4 {
                    let dark = |yy: usize| {
                        x >= 2
                            && x < w + 2
                            && yy >= 2
                            && yy < w + 2
                            && colors[(yy - 2) * w + x - 2] == qrcode::Color::Dark
                    };
                    let top = dark(y);
                    let bottom = dark(y + 1);
                    row.push(match (top, bottom) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        _ => ' ',
                    });
                }
                qrlines.push(Line::from(Span::styled(
                    row,
                    Style::new().fg(Color::Black).bg(Color::White),
                )));
            }
            lines.extend(qrlines);
        }
    } else {
        for (i, f) in app.fields().iter().enumerate() {
            let id = f["id"].as_str().unwrap_or("value");
            let name = f["name"].as_str().unwrap_or(id);
            let raw = app.login_values.get(id).map(String::as_str).unwrap_or("");
            let secret = matches!(f["type"].as_str(), Some("token" | "password" | "cookie"))
                || id.contains("cookie")
                || id.contains("token");
            let value = if secret {
                "•".repeat(raw.chars().count().min(65))
            } else {
                clean(raw)
            };
            lines.push(Line::from(Span::styled(
                name.to_owned(),
                Style::new().fg(t.muted),
            )));
            lines.push(Line::from(Span::styled(
                format!("{} {value}", if i == app.login_field { "▸" } else { " " }),
                Style::new().fg(if i == app.login_field {
                    t.yellow
                } else {
                    t.text
                }),
            )));
            lines.push(Line::from(""));
        }
    }
    if app.login["type"] == "complete" {
        lines.push(Line::from(Span::styled(
            "Connected. Press any key to return to your inbox.",
            Style::new().fg(t.green),
        )));
    } else if app.login["type"] == "error" {
        lines.push(Line::from(Span::styled(
            "enter  retry    esc  accounts",
            Style::new().fg(t.red),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "tab  next field    ctrl+s  submit    esc  back",
            Style::new().fg(t.faint),
        )));
    }
    frame.render_widget(Paragraph::new(lines), r);
}
fn render_attach(frame: &mut Frame, app: &App, t: Theme) {
    let r = modal(frame, 84, 9, "Attach a file", t);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Full path to a local file, up to 20 MB.",
                Style::new().fg(t.muted),
            )),
            Line::from(""),
            Line::from(Span::styled(
                clean(&app.editor.text),
                Style::new().fg(t.yellow),
            )),
        ]),
        r,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_unicode_lines_fit() {
        let lines = wrap_text(
            "a very long 😀message with 🔥 emojis andaverylongwordwithoutspaces",
            12,
        );
        assert!(
            lines
                .iter()
                .all(|l| UnicodeWidthStr::width(l.as_str()) <= 12)
        );
    }
    #[test]
    fn preview_renders_at_small_and_large_sizes() {
        for (w, h) in [(60, 20), (100, 32), (160, 50)] {
            let b = ratatui::backend::TestBackend::new(w, h);
            let mut t = Terminal::new(b).unwrap();
            let app = App::new(None, true);
            t.draw(|f| render(f, &app)).unwrap();
            let text = t
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("chord"));
            assert!(text.contains("teaching"));
        }
    }
    #[test]
    fn contact_views_render_in_normal_and_small_windows() {
        for (w, h) in [(60, 20), (140, 39)] {
            for mode in [
                Mode::Contacts,
                Mode::Details,
                Mode::ContactName,
                Mode::ConfirmContactLink,
            ] {
                let mut app = App::new(None, true);
                let person = serde_json::json!({"id":"person_a","name":"Alex Chen","favorite":true,"identities":[{"key":"@slack_t-u:chord.local","source":"slack","label":"Lab"}],"conversations":[]});
                app.contacts = vec![person.clone()];
                app.details = person.clone();
                app.link_source = person;
                app.contact_editor.set("Alex Chen".into());
                app.mode = mode;
                let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
                terminal.draw(|f| render(f, &app)).unwrap();
                let text = terminal
                    .backend()
                    .buffer()
                    .content()
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>();
                assert!(
                    text.contains("Alex Chen"),
                    "missing contact at {w}x{h}, {mode:?}"
                );
            }
        }
    }
    #[test]
    fn image_preview_and_gallery_render_and_resize() {
        use base64::Engine;
        use std::io::Cursor;
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(40, 20)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(png.get_ref());
        let mut app = App::new(None, true);
        app.focus = Focus::Messages;
        app.messages[app.message_selection].attachments =
            vec![serde_json::json!({"type":"img","fileName":"test.png"})];
        let target = app.image_target().unwrap();
        app.images
            .insert(target.key(), Ok(crate::images::decode(&encoded).unwrap()));
        for (w, h) in [(60, 20), (140, 39), (80, 24)] {
            for mode in [Mode::Browse, Mode::Image] {
                app.mode = mode;
                let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
                terminal.draw(|f| render(f, &app)).unwrap();
                let text = terminal
                    .backend()
                    .buffer()
                    .content()
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>();
                assert!(
                    text.contains("test.png"),
                    "missing image at {w}x{h} {mode:?}"
                );
                assert!(
                    (mode == Mode::Browse && h < 32) || text.contains("40 × 20"),
                    "missing dimensions at {w}x{h} {mode:?}"
                );
            }
        }
    }
    #[test]
    fn discord_server_picker_shows_server_names_in_small_and_normal_windows() {
        for (w, h) in [(60, 20), (140, 39)] {
            let mut app = App::new(None, true);
            app.mode = Mode::Servers;
            app.servers =
                vec![serde_json::json!({"id":"123","name":"SDC Buggy","channelCount":23})];
            let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| render(f, &app)).unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("SDC Buggy"));
            assert!(text.contains("Direct messages"));
        }
    }
}
