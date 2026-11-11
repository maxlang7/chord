mod emoji;
mod icons;
mod links;
mod media;
mod theme;

use crate::{
    api::Api,
    app::{App, Mode, REACTIONS},
    images::Target,
    model::{Conversation, Message, day_label, one_line, source_label, time_label},
};
use eframe::egui::{self, Color32, RichText, Vec2};
use media::{Entry, Event, Media, Request};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use theme::{Palette, Theme};

pub struct Gui {
    pub app: App,
    media: Media,
    theme: Theme,
    search: String,
    gallery: Vec<Target>,
    gallery_index: usize,
    file_busy: bool,
    profile_name: String,
    profile_id: String,
    settings_path: PathBuf,
    scroll_to: Option<String>,
    search_focus: bool,
    message_search: bool,
    reaction_target: Option<(Conversation, Message)>,
    expanded_thread: Option<(String, String)>,
    search_busy: bool,
    search_seq: u64,
    search_results: Value,
}
impl Gui {
    pub fn new(mut app: App, ctx: &egui::Context) -> Self {
        app.native_ui = true;
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "emoji".into(),
            std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
                "../assets/NotoEmoji.ttf"
            ))),
        );
        fonts
            .families
            .insert(egui::FontFamily::Name("emoji".into()), vec!["emoji".into()]);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("emoji".into());
        }
        ctx.set_fonts(fonts);
        let settings_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("private/ui.json");
        let settings: Value = std::fs::read(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok())
            .unwrap_or(Value::Null);
        let theme = match settings["theme"].as_str() {
            Some("light") => Theme::Light,
            Some("dark") => Theme::Dark,
            _ => Theme::Gruvbox,
        };
        theme.apply(ctx);
        Self {
            app,
            media: Media::default(),
            theme,
            search: String::new(),
            gallery: vec![],
            gallery_index: 0,
            file_busy: false,
            profile_name: String::new(),
            profile_id: String::new(),
            settings_path,
            scroll_to: None,
            search_focus: false,
            message_search: false,
            reaction_target: None,
            expanded_thread: None,
            search_busy: false,
            search_seq: 0,
            search_results: Value::Null,
        }
    }
    fn key(&mut self, code: crossterm::event::KeyCode) {
        self.app.key(crossterm::event::KeyEvent::new(
            code,
            crossterm::event::KeyModifiers::NONE,
        ));
    }
    fn save_theme(&mut self, ctx: &egui::Context) {
        self.theme.apply(ctx);
        let value = json!({"theme":match self.theme{Theme::Gruvbox=>"gruvbox",Theme::Dark=>"dark",Theme::Light=>"light"}});
        let result = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            opts.open(&self.settings_path)?
                .write_all(value.to_string().as_bytes())
        })();
        if let Err(e) = result {
            self.app.notice = format!("Could not save theme: {e}");
        }
    }
    fn switch_source(&mut self, index: usize) {
        self.app.source = index;
        self.app.discord_scope = None;
        self.app.server_label.clear();
        self.app.search.clear();
        self.search.clear();
        self.app.selection = 0;
        self.app.unread = false;
        self.app.mode = Mode::Browse;
        self.app.refresh(false);
    }
    fn choose_chat(&mut self, chat: Conversation) {
        self.expanded_thread = None;
        self.app.mode = Mode::Browse;
        self.app.open(chat);
        self.scroll_to = None;
    }
    fn sidebar(&mut self, ctx: &egui::Context, p: Palette) {
        let compact = ctx.content_rect().width() < 1000.;
        egui::SidePanel::left("navigation")
            .exact_width(if compact { 76. } else { 188. })
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(14, 20)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("c·").size(30.).color(p.accent).strong());
                    if !compact {
                        ui.label(RichText::new("chord").size(23.).strong());
                    }
                });
                ui.add_space(25.);
                ui.label(
                    RichText::new(if compact { "INBOX" } else { "YOUR INBOX" })
                        .size(10.)
                        .color(p.muted),
                );
                ui.add_space(8.);
                for (index, label, _short) in [
                    (0, "All accounts", "All"),
                    (1, "Slack", "S"),
                    (2, "Discord", "D"),
                    (3, "WhatsApp", "W"),
                    (4, "iPhone", "P"),
                    (5, "Personal email", "@"),
                    (6, "CMU email", "CMU"),
                ] {
                    let response =
                        icons::button(ui, index, label, compact, self.app.source == index, p);
                    if response.clicked() {
                        if index == 2 {
                            self.app.open_servers();
                        } else {
                            self.switch_source(index);
                        }
                    }
                }
                if self.app.source == 2 && !compact {
                    ui.add_space(5.);
                    if ui.small_button("Browse servers…").clicked() {
                        self.app.open_servers();
                    }
                    if self.app.discord_scope.is_some() {
                        ui.label(RichText::new(&self.app.server_label).small().color(p.muted));
                    }
                }
                ui.add_space(22.);
                ui.separator();
                ui.add_space(10.);
                if ui
                    .add(
                        egui::Button::new(if compact {
                            "People"
                        } else {
                            "Contacts & profiles"
                        })
                        .frame(false),
                    )
                    .clicked()
                {
                    self.app.open_contacts();
                }
                if ui.add(egui::Button::new("Accounts").frame(false)).clicked() {
                    self.app.load_accounts();
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), if compact { 78. } else { 70. }),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.sidebar_footer(ui, p, compact),
                    );
                });
            });
    }
    fn sidebar_footer(&mut self, ui: &mut egui::Ui, p: Palette, compact: bool) {
        ui.separator();
        ui.add_space(8.);
        let accounts = self.app.connections["items"].as_array();
        let online = accounts
            .map(|items| items.iter().filter(|a| a["connected"] == true).count())
            .unwrap_or(0);
        let total = accounts.map(Vec::len).unwrap_or(0);
        let phone = accounts.and_then(|items| items.iter().find(|a| a["id"] == "iphone"));
        let connected = phone.map(|a| a["connected"] == true);
        let state = match connected {
            Some(true) => "Connected",
            Some(false) => "Offline",
            None => "Checking",
        };
        let detail = phone
            .and_then(|a| a["detail"].as_str())
            .unwrap_or("Checking the iPhone Bluetooth connection")
            .to_owned();
        let label = if compact {
            match connected {
                Some(true) => "BT on",
                Some(false) => "BT off",
                None => "BT…",
            }
            .to_owned()
        } else {
            format!("iPhone · {state}")
        };
        if ui
            .add(
                egui::Button::new(RichText::new(label).size(12.).color(
                    if connected == Some(true) {
                        p.accent
                    } else {
                        p.muted
                    },
                ))
                .frame(false),
            )
            .on_hover_text(format!("BlueFerry: {detail}"))
            .clicked()
        {
            self.app.load_accounts();
        }
        ui.horizontal(|ui| {
            if !compact {
                ui.label(
                    RichText::new(if self.app.demo {
                        "Preview".into()
                    } else {
                        format!("{online}/{total} accounts online")
                    })
                    .size(10.)
                    .color(p.muted),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("⚙", |ui| {
                    ui.label(RichText::new("Appearance").small().color(p.muted));
                    let old = self.theme;
                    for theme in [Theme::Gruvbox, Theme::Dark, Theme::Light] {
                        if ui
                            .selectable_value(&mut self.theme, theme, theme.name())
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                    if old != self.theme {
                        self.save_theme(ui.ctx());
                    }
                    ui.separator();
                    if ui.button("Manage accounts").clicked() {
                        self.app.load_accounts();
                        ui.close();
                    }
                })
                .response
                .on_hover_text("Settings");
            });
        });
    }
    fn run_search(&mut self, ctx: &egui::Context) {
        self.search_page(ctx, false);
    }
    fn search_page(&mut self, ctx: &egui::Context, more: bool) {
        self.search_seq += 1;
        let seq = self.search_seq;
        if self.search.trim().is_empty() {
            self.search_results = Value::Null;
            self.search_busy = false;
            return;
        }
        let Some(api) = self.app.api.clone() else {
            return;
        };
        self.search_busy = true;
        let offset = if more {
            self.search_results["nextOffset"].as_u64().unwrap_or(0)
        } else {
            0
        };
        let previous = if more {
            self.search_results["items"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        } else {
            vec![]
        };
        let route = format!(
            "/api/search?query={}&offset={offset}",
            urlencoding::encode(&self.search)
        );
        let tx = self.media.tx.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let result = api
                .get(&route)
                .await
                .map(|mut v| {
                    if more {
                        let mut items = previous;
                        items.extend(v["items"].as_array().cloned().unwrap_or_default());
                        v["items"] = json!(items);
                    }
                    v
                })
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Event::Search(seq, result));
            ctx.request_repaint();
        });
    }
    fn inbox(&mut self, ctx: &egui::Context, p: Palette) {
        egui::SidePanel::left("conversations")
            .default_width(360.)
            .width_range(260. ..=480.)
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(12, 18)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Messages");
                    if self.app.loading {
                        ui.spinner();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("↻").on_hover_text("Refresh").clicked() {
                            self.app.refresh_now();
                        }
                    });
                });
                ui.add_space(10.);
                ui.horizontal(|ui| {
                    if ui.selectable_label(!self.message_search, "Conversations").clicked() { self.message_search = false; }
                    if ui.selectable_label(self.message_search, "Message text").clicked() { self.message_search = true; self.run_search(ctx); }
                });
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text(if self.message_search { "Search messages and email…" } else { "Search conversations…" })
                        .desired_width(f32::INFINITY),
                );
                if self.search_focus {
                    search.request_focus();
                    self.search_focus = false;
                }
                if self.search.is_empty() && !self.app.search.is_empty() {
                    self.app.search.clear();
                    self.app.selection = 0;
                    self.app.refresh(false);
                }
                if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.message_search {
                    self.app.search = self.search.clone();
                    self.app.selection = 0;
                    self.app.refresh(false);
                }
                if !self.search.is_empty() && (ui.small_button("Search").clicked() || (self.message_search && search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))) {
                    if self.message_search { self.run_search(ctx); } else { self.app.search = self.search.clone(); self.app.refresh(false); }
                }
                if self.message_search {
                    if self.search_busy { ui.spinner(); }
                    ui.add(egui::Label::new(RichText::new("Search all saved messages and filenames, including compressed archives. Works offline.").small().color(p.muted)).wrap());
                    let hits = self.search_results["items"].as_array().cloned().unwrap_or_default();
                    egui::ScrollArea::vertical().id_salt("search-results").show(ui, |ui| {
                        if hits.is_empty() && !self.search_busy { ui.label("No results. Enter text and press Search."); }
                        for hit in hits {
                            let Ok(chat) = serde_json::from_value::<Conversation>(hit["conversation"].clone()) else { continue; };
                            let text = hit["message"]["text"].as_str().unwrap_or("");
                            let saved = if hit["archived"] == true { " · archive" } else { " · saved locally" };
                            let label = format!("{} · {}{}\n{}", chat.title, source_label(&chat.source), saved, one_line(text).chars().take(130).collect::<String>());
                            if ui.add_sized([ui.available_width(), 72.], egui::Button::new(label).wrap()).clicked() {
                                self.choose_chat(chat);
                                self.scroll_to = hit["message"]["id"].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
                            }
                        }
                        if self.search_results["hasMore"] == true && !self.search_busy && ui.button("More results").clicked() { self.search_page(ctx, true); }
                    });
                    for error in self.search_results["errors"].as_array().into_iter().flatten() { ui.label(RichText::new(format!("{}: {}", error["source"].as_str().unwrap_or("Account"), error["error"].as_str().unwrap_or("Unavailable"))).small().color(p.muted)); }
                    return;
                }
                ui.add_space(6.);
                ui.horizontal(|ui| {
                    let mut focus = self.app.focus_enabled();
                    if ui.selectable_value(&mut focus, true, "Focus").clicked()
                        || ui.selectable_value(&mut focus, false, "All").clicked()
                    {
                        if self.app.discord_scope.is_some() {
                            self.app.server_focus = focus;
                        } else {
                            self.app.focus_view = focus;
                        }
                        self.app.refresh(false);
                    }
                    if ui.checkbox(&mut self.app.unread, "Unread").changed() {
                        self.app.refresh(false);
                    }
                });
                if let (Some(total), Some(important)) = (self.app.inbox_summary["total"].as_u64(), self.app.inbox_summary["important"].as_u64()) {
                    ui.label(RichText::new(format!("{important} in Focus · {total} total")).small().color(p.muted));
                }
                ui.add_space(10.);
                ui.separator();
                let chats: Vec<Conversation> = self.app.visible().into_iter().cloned().collect();
                egui::ScrollArea::vertical()
                    .id_salt("inbox-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if chats.is_empty() {
                            ui.add_space(25.);
                            ui.label(
                                RichText::new(if self.app.loading {
                                    "Loading your conversations…"
                                } else {
                                    "Nothing here yet. Try All, another account, or refresh."
                                })
                                .color(p.muted),
                            );
                        }
                        if chats.is_empty() && !self.app.loading && self.app.focus_enabled() && self.app.inbox_summary["total"].as_u64().unwrap_or(0) > 0
                            && ui.button(format!("Show all {} conversations", self.app.inbox_summary["total"].as_u64().unwrap_or(0))).clicked() {
                            if self.app.discord_scope.is_some() { self.app.server_focus = false; } else { self.app.focus_view = false; }
                            self.app.refresh(false);
                        }
                        for (chat_index, chat) in chats.iter().enumerate() {
                            if self.conversation_row(ui, chat, p).clicked() {
                                self.app.selection = chat_index;
                                self.choose_chat(chat.clone());
                                ctx.request_repaint();
                            }
                        }
                        if !self.app.inbox_cursors.is_null()
                            && self
                                .app
                                .inbox_cursors
                                .as_object()
                                .is_some_and(|o| !o.is_empty())
                            && ui.button("Load more conversations").clicked()
                        {
                            self.app.refresh(true);
                        }
                    });
            });
    }
    fn conversation_row(
        &mut self,
        ui: &mut egui::Ui,
        chat: &Conversation,
        p: Palette,
    ) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 90.), egui::Sense::click());
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let active = self.app.active.as_ref().is_some_and(|c| c.id == chat.id);
        let source_index = crate::model::SOURCES
            .iter()
            .position(|(id, _, _)| *id == chat.source)
            .unwrap_or(0);
        let color = theme::service_color(source_index);
        if active || response.hovered() {
            ui.painter().rect_filled(
                rect.shrink2(egui::vec2(0., 3.)),
                8.,
                if active {
                    theme::tint(p.panel, color, 0.18)
                } else {
                    theme::tint(p.panel, color, 0.06)
                },
            );
        }
        if active {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(rect.min + egui::vec2(0., 18.), egui::vec2(3., 54.)),
                2.,
                color,
            );
        }
        let avatar_rect =
            egui::Rect::from_min_size(rect.min + egui::vec2(9., 23.), Vec2::splat(44.));
        // The row already owns its layout space. Avatar children must not
        // advance the parent cursor back into that reserved row.
        {
            let mut avatar_ui = ui.new_child(egui::UiBuilder::new().max_rect(avatar_rect));
            let ui = &mut avatar_ui;
            if chat.direct || chat.contact_ids.is_empty() {
                self.avatar(
                    ui,
                    chat.contact_ids
                        .first()
                        .map(String::as_str)
                        .filter(|_| chat.direct),
                    &chat.title,
                    44.,
                    p,
                );
            } else {
                for (index, id) in chat.contact_ids.iter().take(3).enumerate() {
                    let offset = [
                        egui::vec2(0., 0.),
                        egui::vec2(20., 0.),
                        egui::vec2(10., 20.),
                    ][index];
                    let cell =
                        egui::Rect::from_min_size(avatar_rect.min + offset, Vec2::splat(25.));
                    let mut cell_ui = ui.new_child(egui::UiBuilder::new().max_rect(cell));
                    self.avatar(&mut cell_ui, Some(id), &chat.title, 25., p);
                }
            }
        }
        let badge = egui::Rect::from_center_size(
            avatar_rect.right_bottom() - egui::vec2(2., 2.),
            Vec2::splat(20.),
        );
        ui.painter()
            .circle_filled(badge.center(), 11., theme::tint(p.panel, color, 0.22));
        let source = crate::model::SOURCES
            .iter()
            .position(|(id, _, _)| *id == chat.source)
            .unwrap_or(0);
        icons::service(ui.painter(), badge.shrink(3.), source, p.text);
        let text_left = rect.left() + 67.;
        let right = rect.right() - 9.;
        let stamp = time_label(&chat.timestamp);
        let stamp_font = egui::FontId::proportional(11.);
        let stamp_width = ui
            .painter()
            .layout_no_wrap(stamp.clone(), stamp_font.clone(), p.muted)
            .size()
            .x;
        ui.painter().text(
            egui::pos2(right, rect.top() + 20.),
            egui::Align2::RIGHT_TOP,
            stamp,
            stamp_font,
            p.muted,
        );
        let context = chat
            .server_name
            .as_deref()
            .unwrap_or_else(|| source_label(&chat.source));
        let title = if chat.direct {
            chat.title.clone()
        } else {
            format!("{} · {context}", chat.title)
        };
        row_text(
            ui,
            &title,
            egui::pos2(text_left, rect.top() + 20.),
            (right - text_left - stamp_width - 12.).max(28.),
            16.,
            p.text,
        );
        let preview = if chat.preview_sender_name.is_empty() {
            one_line(&chat.preview)
        } else {
            format!("{}: {}", chat.preview_sender_name, one_line(&chat.preview))
        };
        row_text(
            ui,
            &preview,
            egui::pos2(text_left, rect.top() + 50.),
            right - text_left - 18.,
            14.,
            p.muted,
        );
        if chat.unread {
            ui.painter()
                .circle_filled(egui::pos2(right - 4., rect.top() + 59.), 4., p.blue);
        }
        ui.painter().line_segment(
            [
                egui::pos2(text_left, rect.bottom() - 1.),
                egui::pos2(right, rect.bottom() - 1.),
            ],
            egui::Stroke::new(0.5_f32, p.line),
        );
        // Avatar cells are visual parts of this row, not separate navigation.
        let clicked_avatar = ui.input(|input| {
            input.pointer.button_clicked(egui::PointerButton::Primary)
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| avatar_rect.contains(pos))
        }) && ui.ctx().layer_id_at(avatar_rect.center())
            == Some(ui.layer_id());
        if clicked_avatar {
            self.choose_chat(chat.clone());
            ui.ctx().request_repaint();
        }
        response.on_hover_text(format!("{}\n{}", chat.title, context))
    }
    fn avatar(
        &mut self,
        ui: &mut egui::Ui,
        contact: Option<&str>,
        name: &str,
        size: f32,
        _p: Palette,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
        ui.painter()
            .circle_filled(rect.center(), size / 2., theme::avatar_color(name));
        let initials = initials(name);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            initials,
            egui::FontId::proportional(size * 0.34),
            Color32::WHITE,
        );
        if ui.is_rect_visible(rect)
            && let Some(id) = contact
        {
            let key = format!("avatar:{id}");
            self.media.request(
                Request::Avatar(id.to_owned()),
                self.app.api.clone(),
                ui.ctx(),
            );
            if let Some(texture) = self.media.texture(&key) {
                egui::Image::new(&texture)
                    .corner_radius((size / 2.) as u8)
                    .paint_at(ui, rect);
            }
        }
        response.on_hover_text(name)
    }
    fn thread(&mut self, ctx: &egui::Context, p: Palette) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(24, 18)),
            )
            .show(ctx, |ui| {
                let Some(chat) = self.app.active.clone() else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() * 0.32);
                        ui.label(RichText::new("c·").size(58.).color(p.accent));
                        ui.heading("Your conversations");
                        ui.label(
                            RichText::new("Slack, Discord, WhatsApp, iPhone, and email")
                                .color(p.muted),
                        );
                        ui.add_space(15.);
                        ui.label("Choose a conversation to get started.");
                    });
                    return;
                };
                ui.horizontal(|ui| {
                    let photo = chat
                        .contact_ids
                        .first()
                        .map(String::as_str)
                        .filter(|_| chat.direct);
                    if self.avatar(ui, photo, &chat.title, 42., p).clicked()
                        && let Some(id) = photo
                    {
                        self.app.open_contact(json!({"id":id}));
                    }
                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&chat.title).size(20.).strong())
                                .truncate(),
                        );
                        let subtitle = chat
                            .server_name
                            .as_deref()
                            .map(|s| format!("{} · {s}", source_label(&chat.source)))
                            .unwrap_or_else(|| source_label(&chat.source).into());
                        let subtitle = if chat.kind == "archive" {
                            format!("{subtitle} · Local archive · Read only")
                        } else {
                            subtitle
                        };
                        ui.label(RichText::new(subtitle).size(12.).color(p.muted));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button("•••", |ui| {
                            if chat.kind == "archive" {
                                ui.label(
                                    "Saved locally. Open the live conversation to make changes.",
                                );
                                return;
                            }
                            if ui.button("Mark read").clicked() {
                                self.app.set_read_state(true);
                                ui.close();
                            }
                            if ui.button("Mark unread").clicked() {
                                self.app.set_read_state(false);
                                ui.close();
                            }
                            if ui.button("Archive conversation").clicked() {
                                self.app.operation(
                                    "Archived",
                                    Api::conversation(&chat.id, "archive"),
                                    json!({"archived":true}),
                                );
                                ui.close();
                            }
                            ui.separator();
                            for (label, priority) in [
                                ("Pin in Focus", "important"),
                                ("Quiet in Chord", "quiet"),
                                ("Automatic importance", "auto"),
                            ] {
                                if ui.button(label).clicked() {
                                    self.app.operation(
                                        "Importance updated",
                                        "/api/preferences/conversation".into(),
                                        json!({"conversationID":chat.id,"priority":priority}),
                                    );
                                    ui.close();
                                }
                            }
                            ui.separator();
                            for id in &chat.contact_ids {
                                if ui.button("View contact profile").clicked() {
                                    self.app.open_contact(json!({"id":id}));
                                    ui.close();
                                }
                            }
                        });
                    });
                });
                if chat.thread_root_id.is_some() {
                    ui.horizontal(|ui| {
                        if icons::back_button(ui, "Back to channel").clicked() {
                            self.app.back_to_channel();
                        }
                        ui.label(RichText::new("Slack thread").strong());
                    });
                }
                ui.add_space(10.);
                ui.separator();
                egui::TopBottomPanel::bottom("composer")
                    .frame(
                        egui::Frame::new()
                            .fill(p.bg)
                            .inner_margin(egui::Margin::symmetric(0, 12)),
                    )
                    .show_inside(ui, |ui| {
                        self.composer(ui, &chat, p);
                    });
                egui::ScrollArea::vertical()
                    .id_salt(("thread", &chat.id))
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if self.app.message_cursor.is_some()
                            && ui
                                .add_enabled(
                                    !self.app.message_loading,
                                    egui::Button::new("Load earlier messages"),
                                )
                                .clicked()
                        {
                            self.app.load_messages(true);
                        }
                        if self.app.message_loading && self.app.messages.is_empty() {
                            ui.spinner();
                            ui.label("Loading conversation…");
                        }
                        let messages = self.app.messages.clone();
                        let mut previous_day = String::new();
                        let group_threads = chat.source == "slack" && chat.thread_root_id.is_none();
                        for index in message_order(&messages, group_threads) {
                            let message = &messages[index];
                            if group_threads
                                && let Some(root) = &message.thread_root_id
                                && messages.iter().any(|m| &m.id == root)
                                && self.expanded_thread.as_ref()
                                    != Some(&(chat.id.clone(), root.clone()))
                            {
                                continue;
                            }
                            let day = day_label(&message.timestamp);
                            if day != previous_day {
                                previous_day = day.clone();
                                ui.add_space(14.);
                                ui.horizontal(|ui| {
                                    ui.separator();
                                    ui.label(RichText::new(day).size(11.).color(p.muted));
                                    ui.separator();
                                });
                                ui.add_space(8.);
                            }
                            if message.thread_root_id.is_some() {
                                let reply = egui::Frame::new()
                                    .inner_margin(egui::Margin {
                                        left: 16,
                                        ..Default::default()
                                    })
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        self.message(ui, &chat, message, index, p);
                                    });
                                let x = reply.response.rect.left() + 5.;
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(x, reply.response.rect.top()),
                                        egui::pos2(x, reply.response.rect.bottom()),
                                    ],
                                    egui::Stroke::new(
                                        1.5_f32,
                                        Color32::from_gray(128).gamma_multiply(0.55),
                                    ),
                                );
                            } else {
                                self.message(ui, &chat, message, index, p);
                            }
                            ui.add_space(9.);
                        }
                        if messages.is_empty() && !self.app.message_loading {
                            ui.label(RichText::new("No messages loaded yet.").color(p.muted));
                        }
                        ui.add_space(10.);
                    });
            });
    }
    fn message(
        &mut self,
        ui: &mut egui::Ui,
        chat: &Conversation,
        m: &Message,
        index: usize,
        p: Palette,
    ) {
        let has_thread = chat.source == "slack"
            && chat.thread_root_id.is_none()
            && m.thread_root_id.is_none()
            && (m.reply_count > 0
                || self
                    .app
                    .messages
                    .iter()
                    .any(|reply| reply.thread_root_id.as_ref() == Some(&m.id)));
        let mut thread_clicked = false;
        let response = ui
            .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
                egui::Frame::new()
                    .fill(if m.is_sender { p.own } else { p.bg })
                    .corner_radius(12)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            if self
                                .avatar(ui, m.sender_contact_id.as_deref(), &m.sender_name, 34., p)
                                .clicked()
                                && let Some(id) = &m.sender_contact_id
                            {
                                self.app.open_contact(json!({"id":id}));
                            }
                            ui.vertical(|ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (name_rect, name_response) = ui.allocate_exact_size(
                                        egui::vec2((ui.available_width() - 112.).max(24.), 22.),
                                        egui::Sense::click(),
                                    );
                                    row_text(
                                        ui,
                                        &m.sender_name,
                                        name_rect.min + egui::vec2(0., 2.),
                                        name_rect.width(),
                                        14.,
                                        if m.is_sender {
                                            p.accent
                                        } else {
                                            theme::avatar_color(&m.sender_name)
                                        },
                                    );
                                    if name_response.clicked()
                                        && let Some(id) = &m.sender_contact_id
                                    {
                                        self.app.open_contact(json!({"id":id}));
                                    }
                                    ui.label(
                                        RichText::new(time_label(&m.timestamp))
                                            .size(11.)
                                            .color(p.muted),
                                    );
                                    if m.historical_version {
                                        ui.label(
                                            RichText::new("Saved version").small().color(p.muted),
                                        );
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.menu_button("•••", |ui| {
                                                if chat.source == "slack"
                                                    && chat.kind == "chat"
                                                    && chat.thread_root_id.is_none()
                                                    && ui.button("View full thread").clicked()
                                                {
                                                    self.app.open_thread(m);
                                                    ui.close();
                                                }
                                                if chat.can_react {
                                                    self.reaction_menu(ui, chat, m, p);
                                                }
                                                if ui.button("Copy message").clicked() {
                                                    ui.ctx().copy_text(m.text.clone());
                                                    ui.close();
                                                }
                                                if chat.can_reply && ui.button("Reply").clicked() {
                                                    self.reply_to(index);
                                                    ui.close();
                                                }
                                                if m.is_sender
                                                    && chat.kind == "chat"
                                                    && ui.button("Edit message").clicked()
                                                {
                                                    self.app.message_selection = index;
                                                    self.app.editor.set(m.text.clone());
                                                    self.app.edit_message = Some(m.id.clone());
                                                    self.app.mode = Mode::Compose;
                                                    ui.close();
                                                }
                                                if let Some(id) = &m.sender_contact_id
                                                    && ui.button("View profile").clicked()
                                                {
                                                    self.app.open_contact(json!({"id":id}));
                                                    ui.close();
                                                }
                                            });
                                        },
                                    );
                                });
                                if let Some(reply_id) = &m.linked_message_id
                                    && m.thread_root_id.as_ref() != Some(reply_id)
                                {
                                    let reply = self
                                        .app
                                        .messages
                                        .iter()
                                        .find(|msg| &msg.id == reply_id)
                                        .cloned();
                                    let label = reply
                                        .as_ref()
                                        .map(|r| {
                                            format!("{}  ·  {}", r.sender_name, one_line(&r.text))
                                        })
                                        .unwrap_or_else(|| "Reply to an earlier message".into());
                                    let card = egui::Frame::new()
                                        .fill(p.panel)
                                        .inner_margin(8)
                                        .corner_radius(5)
                                        .show(ui, |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(label).size(12.).color(p.blue),
                                                )
                                                .truncate(),
                                            );
                                        });
                                    if ui
                                        .interact(
                                            card.response.rect,
                                            ui.id().with(("reply", &m.id)),
                                            egui::Sense::click(),
                                        )
                                        .clicked()
                                    {
                                        self.scroll_to = Some(reply_id.clone());
                                    }
                                }
                                if chat.kind == "email" {
                                    ui.label(RichText::new(&m.subject).strong());
                                    ui.label(
                                        RichText::new(format!("From {}", m.from))
                                            .size(12.)
                                            .color(p.muted),
                                    );
                                }
                                if !m.text.is_empty() {
                                    thread_clicked |= links::text(ui, &m.text);
                                }
                                if has_thread {
                                    let count = m.reply_count.max(
                                        self.app
                                            .messages
                                            .iter()
                                            .filter(|reply| {
                                                reply.thread_root_id.as_ref() == Some(&m.id)
                                            })
                                            .count(),
                                    );
                                    let expanded = self.expanded_thread.as_ref()
                                        == Some(&(chat.id.clone(), m.id.clone()));
                                    thread_clicked |= ui
                                        .add(
                                            egui::Label::new(
                                                RichText::new(format!(
                                                    "{count} {} · {}",
                                                    if count == 1 { "reply" } else { "replies" },
                                                    if expanded {
                                                        "click to collapse"
                                                    } else {
                                                        "click to expand"
                                                    }
                                                ))
                                                .small()
                                                .color(p.muted),
                                            )
                                            .sense(egui::Sense::click()),
                                        )
                                        .clicked();
                                }
                                for (attachment_index, a) in m.attachments.iter().enumerate() {
                                    let target = Target {
                                        chat: chat.id.clone(),
                                        message: m.id.clone(),
                                        index: attachment_index,
                                        attachment: a.clone(),
                                    };
                                    if crate::images::is_image(a) {
                                        if self
                                            .image(
                                                ui,
                                                &target,
                                                egui::vec2(ui.available_width().min(440.), 300.),
                                                p,
                                            )
                                            .clicked()
                                        {
                                            self.gallery = m
                                                .attachments
                                                .iter()
                                                .enumerate()
                                                .filter(|(_, a)| crate::images::is_image(a))
                                                .map(|(index, a)| Target {
                                                    chat: chat.id.clone(),
                                                    message: m.id.clone(),
                                                    index,
                                                    attachment: a.clone(),
                                                })
                                                .collect();
                                            self.gallery_index = self
                                                .gallery
                                                .iter()
                                                .position(|t| t.index == attachment_index)
                                                .unwrap_or(0);
                                        }
                                    } else {
                                        if ui
                                            .button(
                                                RichText::new(format!(
                                                    "↓ Save {}",
                                                    a["fileName"].as_str().unwrap_or("Attachment")
                                                ))
                                                .color(p.blue),
                                            )
                                            .clicked()
                                        {
                                            self.app.notice = "Downloading attachment…".into();
                                            self.media.download_file(
                                                target,
                                                self.app.api.clone(),
                                                ui.ctx(),
                                            );
                                        }
                                    }
                                }
                                let mut reactions = BTreeMap::<String, (usize, bool)>::new();
                                for reaction in &m.reactions {
                                    let key =
                                        reaction["reactionKey"].as_str().unwrap_or("?").to_owned();
                                    let e = reactions.entry(key).or_default();
                                    e.0 += 1;
                                    e.1 |= reaction["participantID"].as_str()
                                        == self.app.status["userID"].as_str()
                                        && reaction["participantID"].is_string();
                                }
                                if !reactions.is_empty() {
                                    ui.horizontal_wrapped(|ui| {
                                        for (key, (count, mine)) in reactions {
                                            if ui
                                                .add_enabled(
                                                    chat.can_react,
                                                    emoji::button(ui, &key, &count.to_string())
                                                        .fill(if mine { p.own } else { p.raised })
                                                        .corner_radius(10),
                                                )
                                                .on_hover_text(if mine {
                                                    "Remove your reaction"
                                                } else {
                                                    "React"
                                                })
                                                .clicked()
                                            {
                                                self.react(chat, m, &key, mine);
                                            }
                                        }
                                    });
                                }
                            });
                        });
                    })
                    .response
            })
            .response;
        if has_thread {
            response
                .clone()
                .on_hover_text("Click to expand or collapse replies");
            if (response.clicked() || thread_clicked) && !response.double_clicked() {
                let target = (chat.id.clone(), m.id.clone());
                if self.expanded_thread.as_ref() == Some(&target) {
                    self.expanded_thread = None;
                } else {
                    self.expanded_thread = Some(target);
                    self.app.load_inline_thread(&m.id);
                }
            }
        }
        if chat.can_react {
            let activation_pos = ui.input(|input| {
                input.pointer.interact_pos().filter(|pos| {
                    response.rect.contains(*pos)
                        && (input
                            .pointer
                            .button_double_clicked(egui::PointerButton::Primary)
                            || input.pointer.button_clicked(egui::PointerButton::Secondary))
                })
            });
            let activated =
                activation_pos.is_some_and(|pos| ui.ctx().layer_id_at(pos) == Some(ui.layer_id()));
            if activated {
                self.reaction_target = Some((chat.clone(), m.clone()));
            }
        }
        if self.scroll_to.as_ref() == Some(&m.id) {
            response.scroll_to_me(Some(egui::Align::Center));
            self.scroll_to = None;
        }
    }
    fn reply_to(&mut self, index: usize) {
        self.app.message_selection = index;
        self.app.begin_compose(true);
    }
    fn react(&mut self, chat: &Conversation, m: &Message, key: &str, remove: bool) {
        if !chat.can_react {
            return;
        }
        self.app.operation(
            "Reaction queued",
            Api::conversation(&chat.id, "reaction"),
            json!({"messageID":m.id,"reactionKey":key,"remove":remove}),
        );
    }
    fn reaction_picker(&mut self, ctx: &egui::Context) {
        let Some((chat, message)) = self.reaction_target.clone() else {
            return;
        };
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.reaction_target = None;
            return;
        }
        let mut open = true;
        let mut picked = false;
        egui::Window::new("React to message")
            .id(egui::Id::new("reaction_picker"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(300.)
            .show(ctx, |ui| {
                ui.label(RichText::new(&chat.title).strong());
                ui.label(&message.sender_name);
                ui.add(egui::Label::new(one_line(&message.text)).truncate());
                ui.label("Choose an emoji. Select your existing reaction to remove it.");
                ui.horizontal_wrapped(|ui| {
                    for key in REACTIONS {
                        let mine = message.reactions.iter().any(|r| {
                            r["reactionKey"] == *key
                                && r["participantID"].is_string()
                                && r["participantID"] == self.app.status["userID"]
                        });
                        if ui.add(emoji::button(ui, key, "").selected(mine)).clicked() {
                            self.react(&chat, &message, key, mine);
                            picked = true;
                        }
                    }
                });
            });
        if !open || picked {
            self.reaction_target = None;
        }
    }
    fn reaction_menu(&mut self, ui: &mut egui::Ui, chat: &Conversation, m: &Message, p: Palette) {
        ui.menu_button(RichText::new("☺").color(p.muted), |ui| {
            ui.set_max_width(260.);
            ui.horizontal_wrapped(|ui| {
                for key in REACTIONS {
                    if ui.add(emoji::button(ui, key, "")).clicked() {
                        self.react(chat, m, key, false);
                        ui.close();
                    }
                }
            });
        });
    }
    fn image(
        &mut self,
        ui: &mut egui::Ui,
        target: &Target,
        max: Vec2,
        p: Palette,
    ) -> egui::Response {
        let key = target.key();
        let dimensions = match self.media.cache.get(&key) {
            Some(Entry::Ready { data, .. }) => Some(data.dimensions),
            _ => target.attachment["size"]["width"]
                .as_u64()
                .zip(target.attachment["size"]["height"].as_u64())
                .map(|(w, h)| (w as u32, h as u32)),
        };
        let (w, h) = dimensions.unwrap_or((640, 400));
        let scale = (max.x / w.max(1) as f32)
            .min(max.y / h.max(1) as f32)
            .min(1.);
        let size = egui::vec2((w as f32 * scale).max(40.), (h as f32 * scale).max(32.));
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        if ui.is_rect_visible(rect) {
            self.media.request(
                Request::Image(target.clone()),
                self.app.api.clone(),
                ui.ctx(),
            );
            if let Some(texture) = self.media.texture(&key) {
                egui::Image::new(&texture)
                    .corner_radius(10)
                    .paint_at(ui, rect);
            } else {
                ui.painter().rect_filled(rect, 10, p.panel);
                let label = match self.media.cache.get(&key) {
                    Some(Entry::Failed(_)) => "Image unavailable · click to inspect",
                    _ => "Loading image…",
                };
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(13.),
                    p.muted,
                );
            }
        }
        response.on_hover_text(target.name())
    }
    fn composer(&mut self, ui: &mut egui::Ui, chat: &Conversation, p: Palette) {
        if let Some(reply) = self.app.reply.clone() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Reply to {}", reply.sender_name))
                        .small()
                        .color(p.blue),
                );
                ui.add(
                    egui::Label::new(RichText::new(one_line(&reply.text)).small().color(p.muted))
                        .truncate(),
                );
                if ui.small_button("×").clicked() {
                    self.app.reply = None;
                }
            });
        }
        if self.app.edit_message.is_some() {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Editing your message").small().color(p.blue));
                if ui.small_button("Cancel edit").clicked() {
                    self.app.edit_message = None;
                    self.app.editor.set(String::new());
                }
            });
        }
        if let Some(a) = self.app.pending_attachment.clone() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "▧ {}",
                        a["fileName"].as_str().unwrap_or("Attachment")
                    ))
                    .small()
                    .color(p.blue),
                );
                if ui.small_button("Remove").clicked() {
                    self.app.pending_attachment = None;
                }
            });
        }
        let enabled = chat.can_send && !self.file_busy && !self.app.send_busy;
        egui::Frame::new()
            .fill(p.panel)
            .stroke(egui::Stroke::new(1.0_f32, p.line))
            .corner_radius(12)
            .inner_margin(12)
            .show(ui, |ui| {
                let before = self.app.editor.text.clone();
                let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
                    let mut job = egui::text::LayoutJob::default();
                    job.wrap.max_width = width;
                    emoji::append(
                        &mut job,
                        text.as_str(),
                        egui::TextFormat {
                            font_id: egui::TextStyle::Body.resolve(ui.style()),
                            color: ui.visuals().text_color(),
                            ..Default::default()
                        },
                    );
                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                };
                let edit = ui
                    .add_enabled_ui(enabled, |ui| {
                        egui::TextEdit::multiline(&mut self.app.editor.text)
                            .hint_text(if enabled {
                                "Write a message…"
                            } else if chat.kind == "archive" {
                                "Local archive is read only"
                            } else {
                                "Sending is unavailable for this conversation"
                            })
                            .desired_rows(2)
                            .desired_width(f32::INFINITY)
                            .frame(false)
                            .layouter(&mut layouter)
                            .return_key(egui::KeyboardShortcut::new(
                                egui::Modifiers::SHIFT,
                                egui::Key::Enter,
                            ))
                            .show(ui)
                    })
                    .inner;
                emoji::paint_clipped(ui, &edit.galley, edit.galley_pos, edit.text_clip_rect);
                let response = edit.response;
                self.app.editor.cursor = self.app.editor.text.len();
                if response.changed() && self.app.mode == Mode::Browse {
                    self.app.mode = Mode::Compose;
                }
                let send_key = response.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                ui.horizontal(|ui| {
                    if chat.kind == "chat"
                        && ui
                            .add_enabled(
                                !self.file_busy,
                                egui::Button::new("＋ Attach").frame(false),
                            )
                            .clicked()
                    {
                        self.pick_file(ui.ctx());
                    }
                    ui.label(
                        RichText::new("Shift + Enter for a new line")
                            .size(10.)
                            .color(p.muted),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let can_send = enabled
                            && (!self.app.editor.text.trim().is_empty()
                                || self.app.pending_attachment.is_some())
                            && self.app.mode != Mode::Confirm;
                        if ui
                            .add_enabled(
                                can_send,
                                egui::Button::new(
                                    RichText::new(if self.app.edit_message.is_some() {
                                        "Save"
                                    } else {
                                        "Send"
                                    })
                                    .color(p.bg),
                                )
                                .fill(p.accent)
                                .corner_radius(7),
                            )
                            .clicked()
                            || (can_send && send_key)
                        {
                            self.app.prepare_send();
                        }
                    });
                });
                if before != self.app.editor.text
                    && self.app.editor.text.is_empty()
                    && self.app.mode == Mode::Compose
                {
                    self.app.mode = Mode::Browse;
                }
            });
    }
    fn pick_file(&mut self, ctx: &egui::Context) {
        self.file_busy = true;
        let tx = self.media.tx.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            if let Some(file) = rfd::AsyncFileDialog::new()
                .set_title("Attach to Chord")
                .pick_file()
                .await
            {
                let _ = tx.send(Event::File(file.path().to_owned()));
            } else {
                let _ = tx.send(Event::Notice("Attachment selection cancelled".into()));
            }
            ctx.request_repaint();
        });
    }
    fn dialogs(&mut self, ctx: &egui::Context, p: Palette) {
        match self.app.mode {
            Mode::Confirm => {
                egui::Modal::new(egui::Id::new("review-send")).show(ctx,|ui|{
     ui.set_width(440.);ui.heading(if self.app.confirm_action=="edit"{"Save this edit?"}else{"Send this message?"});
     ui.label(RichText::new(self.app.active.as_ref().map(|c|c.title.clone()).unwrap_or_default()).color(p.blue));
     let body=self.app.confirm_body.clone().unwrap_or(Value::Null);
     if let Some(to)=body["to"].as_str(){ui.label(format!("To: {to}"));ui.label(body["subject"].as_str().unwrap_or(""));}
     ui.add_space(10.);egui::ScrollArea::vertical().max_height(240.).show(ui,|ui|{ui.add(egui::Label::new(body["text"].as_str().unwrap_or("")).wrap().selectable(true));});
     if let Some(a)=self.app.pending_attachment.as_ref(){ui.label(format!("Attachment: {}",a["fileName"].as_str().unwrap_or("File")));}
     if body["confirmGroup"]==true{ui.colored_label(p.accent,"This sends to the displayed phone group. Chord will recheck its recipients.");}
     ui.add_space(15.);ui.horizontal(|ui|{
      if ui.button("Keep editing").clicked(){self.app.confirm_body=None;self.app.mode=Mode::Compose;}
      if ui.add(egui::Button::new(RichText::new("Send").color(p.bg)).fill(p.accent)).clicked(){self.app.confirm();}
     });
    });
            }
            Mode::Servers => {
                let mut open = true;
                egui::Window::new("Discord servers")
                    .open(&mut open)
                    .default_width(430.)
                    .default_height(480.)
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.app.server_query)
                                .hint_text("Find a server…")
                                .desired_width(f32::INFINITY),
                        );
                        if ui.checkbox(&mut self.app.show_other_servers, "Show other servers").changed() {
                            self.app.server_selection = 0;
                        }
                        ui.label(RichText::new("Search finds every server. Quiet channels never add unread badges.").small());
                        let entries = self.app.visible_servers();
                        egui::ScrollArea::vertical()
                            .max_height(480.)
                            .show(ui, |ui| {
                                for (index, s) in entries.iter().enumerate() {
                                    let label = s["name"].as_str().unwrap_or("Server");
                                    let count = s["channelCount"].as_u64();
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 42.],
                                            egui::Button::new(if let Some(count) = count {
                                                format!("{label}    {count} channels")
                                            } else {
                                                label.into()
                                            })
                                            .frame(false),
                                        )
                                        .clicked()
                                    {
                                        self.app.server_selection = index;
                                        self.key(crossterm::event::KeyCode::Enter);
                                        self.search.clear();
                                    }
                                }
                            });
                    });
                if !open {
                    self.app.mode = Mode::Browse;
                }
            }
            Mode::Contacts
            | Mode::ContactLink
            | Mode::Details
            | Mode::ContactName
            | Mode::ConfirmContactLink => self.contacts_window(ctx, p),
            Mode::Accounts | Mode::Login => self.accounts_window(ctx, p),
            _ => {}
        }
        self.gallery_window(ctx, p);
    }
    fn contacts_window(&mut self, ctx: &egui::Context, p: Palette) {
        let mut open = true;
        egui::Window::new("Contacts & profiles")
            .open(&mut open)
            .default_width(440.)
            .default_height(550.)
            .collapsible(false)
            .show(ctx, |ui| match self.app.mode {
                Mode::Contacts | Mode::ContactLink => {
                    if self.app.mode == Mode::ContactLink {
                        ui.label(
                            RichText::new("Choose the same person on another account").strong(),
                        );
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.app.contact_query)
                            .hint_text("Search names, networks, or identities…")
                            .desired_width(f32::INFINITY),
                    );
                    let people: Vec<Value> =
                        self.app.visible_contacts().into_iter().cloned().collect();
                    ui.label(
                        RichText::new(format!("{} contacts", people.len()))
                            .small()
                            .color(p.muted),
                    );
                    egui::ScrollArea::vertical().max_height(520.).show_rows(
                        ui,
                        56.,
                        people.len(),
                        |ui, range| {
                            for index in range {
                                let person = &people[index];
                                let name = person["name"].as_str().unwrap_or("Contact");
                                let id = person["id"].as_str();
                                let row = ui
                                    .horizontal(|ui| {
                                        self.avatar(ui, id, name, 36., p);
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{}{}",
                                                    name,
                                                    if person["favorite"] == true {
                                                        "  ★"
                                                    } else {
                                                        ""
                                                    }
                                                ))
                                                .strong(),
                                            );
                                            let identities = person["identities"]
                                                .as_array()
                                                .cloned()
                                                .unwrap_or_default();
                                            ui.label(
                                                RichText::new(
                                                    identities
                                                        .iter()
                                                        .filter_map(|i| i["source"].as_str())
                                                        .collect::<Vec<_>>()
                                                        .join(" · "),
                                                )
                                                .small()
                                                .color(p.muted),
                                            );
                                        });
                                    })
                                    .response;
                                if ui
                                    .interact(row.rect, ui.id().with(id), egui::Sense::click())
                                    .clicked()
                                {
                                    self.app.contact_selection = index;
                                    self.key(crossterm::event::KeyCode::Enter);
                                }
                            }
                        },
                    );
                }
                Mode::Details | Mode::ContactName => {
                    let person = self.app.details.clone();
                    let id = person["id"].as_str().unwrap_or("");
                    let name = person["name"].as_str().unwrap_or("Contact");
                    if self.profile_id != id {
                        self.profile_id = id.into();
                        self.profile_name = name.into();
                    }
                    ui.horizontal(|ui| {
                        if ui.small_button("← Contacts").clicked() {
                            self.app.open_contacts();
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(if person["favorite"] == true {
                                    "★ Favorite"
                                } else {
                                    "☆ Favorite"
                                })
                                .clicked()
                            {
                                self.app.operation(
                                    "contact_saved",
                                    format!("/api/contacts/{id}"),
                                    json!({"favorite":person["favorite"]!=true}),
                                );
                            }
                        });
                    });
                    ui.add_space(15.);
                    ui.vertical_centered(|ui| {
                        self.avatar(ui, Some(id), name, 82., p);
                        ui.add_space(8.);
                        ui.heading(name);
                    });
                    ui.add_space(15.);
                    ui.label(RichText::new("Display name").small().color(p.muted));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.profile_name).desired_width(280.),
                        );
                        if ui.button("Save").clicked() {
                            self.app.operation(
                                "contact_saved",
                                format!("/api/contacts/{id}"),
                                json!({"name":self.profile_name.trim()}),
                            );
                        }
                    });
                    ui.label(
                        RichText::new("Used across your linked accounts in Chord.")
                            .small()
                            .color(p.muted),
                    );
                    ui.add_space(12.);
                    ui.separator();
                    for identity in person["identities"].as_array().into_iter().flatten() {
                        let source = identity["source"].as_str().unwrap_or("Account");
                        let label = identity["label"]
                            .as_str()
                            .or_else(|| identity["handle"].as_str())
                            .unwrap_or("");
                        ui.label(
                            RichText::new(format!("{}  {}", source_label(source), label)).strong(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(identity["key"].as_str().unwrap_or(""))
                                    .small()
                                    .color(p.muted),
                            )
                            .wrap()
                            .selectable(true),
                        );
                    }
                    ui.add_space(10.);
                    ui.horizontal(|ui| {
                        if ui.button("Link another identity…").clicked() {
                            self.app.mode = Mode::Details;
                            self.key(crossterm::event::KeyCode::Char('m'));
                        }
                        if ui.small_button("Undo last link").clicked() {
                            self.app.operation(
                                "contacts_undone",
                                "/api/contacts/undo-merge".into(),
                                json!({}),
                            );
                        }
                    });
                    ui.add_space(10.);
                    ui.separator();
                    ui.label(RichText::new("Conversations").small().color(p.muted));
                    for c in person["conversations"].as_array().into_iter().flatten() {
                        if let Ok(chat) = serde_json::from_value::<Conversation>(c.clone())
                            && ui
                                .button(format!("{} · {}", chat.title, source_label(&chat.source)))
                                .clicked()
                        {
                            self.choose_chat(chat);
                        }
                    }
                }
                Mode::ConfirmContactLink => {
                    ui.heading("Link these contacts?");
                    let source = self.app.link_source.clone();
                    let target = self.app.details.clone();
                    for person in [&source, &target] {
                        ui.label(
                            RichText::new(person["name"].as_str().unwrap_or("Contact")).strong(),
                        );
                        for i in person["identities"].as_array().into_iter().flatten() {
                            ui.label(
                                RichText::new(i["key"].as_str().unwrap_or(""))
                                    .small()
                                    .color(p.muted),
                            );
                        }
                        ui.add_space(10.);
                    }
                    ui.label(
                        "Confirm these accounts belong to the same person. You can undo this link.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.key(crossterm::event::KeyCode::Esc);
                        }
                        if ui.button("Link contacts").clicked() {
                            self.key(crossterm::event::KeyCode::Enter);
                        }
                    });
                }
                _ => {}
            });
        if !open {
            self.app.mode = Mode::Browse;
        }
    }
    fn accounts_window(&mut self, ctx: &egui::Context, p: Palette) {
        let mut open = true;
        egui::Window::new("Connected accounts")
            .open(&mut open)
            .default_width(470.)
            .collapsible(false)
            .show(ctx, |ui| {
                if self.app.mode == Mode::Accounts {
                    ui.label(
                        RichText::new("Your accounts, directly connected")
                            .size(18.)
                            .strong(),
                    );
                    ui.label(
                        RichText::new("Existing sessions are reused. No Beeper account is needed.")
                            .small()
                            .color(p.muted),
                    );
                    ui.add_space(12.);
                    if ui.button("Refresh status").clicked() {
                        self.app.load_connections();
                        self.app.load_archive_status();
                    }
                    let items = self.app.connections["items"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    if items.is_empty() {
                        ui.spinner();
                        ui.label("Checking connections…");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(490.)
                        .show(ui, |ui| {
                            ui.collapsing("Local archive", |ui| {
                                let status = &self.app.archive_status;
                                if status.is_null() {
                                    ui.label(if self.app.demo {
                                        "Archive status is unavailable in the design preview."
                                    } else {
                                        "Loading archive status…"
                                    });
                                } else if let Some(error) = status["statusError"].as_str() {
                                    ui.colored_label(p.muted, format!("Archive status unavailable: {error}"));
                                } else {
                                    ui.label(format!(
                                        "{} messages in {} conversations · {} saved versions",
                                        status["messages"].as_u64().unwrap_or(0),
                                        status["conversations"].as_u64().unwrap_or(0),
                                        status["records"]["versions"].as_u64().unwrap_or(0)
                                    ));
                                    let stored: u64 = ["records", "blobs", "resources"].iter()
                                        .map(|key| status[key]["storedBytes"].as_u64().unwrap_or(0)).sum();
                                    ui.label(format!("Saved payloads: {:.1} MiB · {} downloaded files",
                                        stored as f64 / 1_048_576.,
                                        status["resources"]["files"].as_u64().unwrap_or(0)));
                                    ui.label(format!("Migration: {} · Worker: {}",
                                        status["migration"]["state"].as_str().unwrap_or("pending"),
                                        status["worker"].as_str().unwrap_or("unknown")));
                                    if let Some(error) = status["lastError"].as_str() {
                                        ui.colored_label(p.muted, format!("Archive needs attention: {error}"));
                                    }
                                    ui.add(egui::Label::new(RichText::new(
                                        "Fetched content is retained indefinitely. Older payloads are compressed after 30 days. Payload size excludes database indexes and recovery copies."
                                    ).small().color(p.muted)).wrap());
                                }
                            });
                            ui.separator();
                            for account in items {
                                let id = account["id"].as_str().unwrap_or("");
                                let connected = account["connected"] == true;
                                ui.horizontal(|ui| {
                                    ui.colored_label(
                                        if connected { p.accent } else { p.muted },
                                        if connected { "Online" } else { "Offline" },
                                    );
                                    ui.label(
                                        RichText::new(account["name"].as_str().unwrap_or(id))
                                            .strong(),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if account["canConnect"] == true
                                                && ui
                                                    .button(if connected {
                                                        "Reconnect"
                                                    } else {
                                                        "Connect"
                                                    })
                                                    .clicked()
                                            {
                                                self.app.start_login(id.to_owned(), None);
                                            }
                                        },
                                    );
                                });
                                if let Some(email) = account["email"].as_str() {
                                    ui.label(email);
                                }
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(account["detail"].as_str().unwrap_or(""))
                                            .small()
                                            .color(p.muted),
                                    )
                                    .wrap(),
                                );
                                ui.add_space(8.);
                                ui.separator();
                            }
                        });
                    for error in &self.app.errors {
                        ui.add(
                            egui::Label::new(RichText::new(error).small().color(p.muted)).wrap(),
                        );
                    }
                } else {
                    if ui.small_button("← Accounts").clicked() {
                        self.app.mode = Mode::Accounts;
                    }
                    ui.heading(&self.app.login_id);
                    links::text(ui,
                        self.app.login["instructions"]
                            .as_str()
                            .unwrap_or("Follow the account's sign-in instructions."),
                    );
                    if self.app.login_busy {
                        ui.spinner();
                    }
                    let code = self.app.login["display_and_wait"]["data"]
                        .as_str()
                        .or_else(|| self.app.login["qr"].as_str())
                        .map(str::to_owned);
                    if let Some(code) = code
                        && let Ok(qr) = qrcode::QrCode::new(&code)
                    {
                        let modules = qr.width();
                        let scale = (260. / (modules + 8) as f32).floor().max(2.);
                        let side = (modules + 8) as f32 * scale;
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(side), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 4, Color32::WHITE);
                        for y in 0..modules {
                            for x in 0..modules {
                                if qr[(x, y)] == qrcode::Color::Dark {
                                    ui.painter().rect_filled(
                                        egui::Rect::from_min_size(
                                            rect.min
                                                + egui::vec2(
                                                    (x + 4) as f32 * scale,
                                                    (y + 4) as f32 * scale,
                                                ),
                                            Vec2::splat(scale),
                                        ),
                                        0,
                                        Color32::BLACK,
                                    );
                                }
                            }
                        }
                    }
                    for field in self.app.fields() {
                        let id = field["id"].as_str().unwrap_or("value").to_owned();
                        ui.label(field["name"].as_str().unwrap_or(&id));
                        let secret = matches!(
                            field["type"].as_str(),
                            Some("token" | "password" | "cookie")
                        ) || id.contains("token")
                            || id.contains("cookie");
                        let text = self.app.login_values.entry(id).or_default();
                        ui.add(
                            egui::TextEdit::singleline(text)
                                .password(secret)
                                .desired_width(f32::INFINITY),
                        );
                    }
                    ui.add_space(10.);
                    if self.app.login["type"] == "complete" {
                        ui.colored_label(p.accent, "Connected");
                        if ui.button("Done").clicked() {
                            self.app.mode = Mode::Browse;
                        }
                    } else if self.app.login["type"] == "error" {
                        ui.label(
                            self.app.login["error"]
                                .as_str()
                                .unwrap_or("Sign-in did not complete."),
                        );
                        if ui.button("Retry").clicked() {
                            self.app.start_login(self.app.login_id.clone(), None);
                        }
                    } else if !self.app.fields().is_empty()
                        && ui
                            .add_enabled(!self.app.login_busy, egui::Button::new("Continue"))
                            .clicked()
                    {
                        self.app.submit_login();
                    }
                }
            });
        if !open {
            self.app.mode = Mode::Browse;
        }
    }
    fn gallery_window(&mut self, ctx: &egui::Context, p: Palette) {
        if self.gallery.is_empty() {
            return;
        }
        self.gallery_index = self.gallery_index.min(self.gallery.len() - 1);
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.gallery.clear();
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            self.gallery_index = (self.gallery_index + 1).min(self.gallery.len() - 1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            self.gallery_index = self.gallery_index.saturating_sub(1);
        }
        let target = self.gallery[self.gallery_index].clone();
        let key = target.key();
        let max = ctx.content_rect().size() - egui::vec2(120., 180.);
        let mut open = true;
        egui::Window::new("Image")
            .id(egui::Id::new("image-gallery"))
            .open(&mut open)
            .collapsible(false)
            .default_size(egui::vec2(760., 600.))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(target.name()).strong());
                    ui.label(
                        RichText::new(format!(
                            "{} / {}",
                            self.gallery_index + 1,
                            self.gallery.len()
                        ))
                        .small()
                        .color(p.muted),
                    );
                });
                self.image(
                    ui,
                    &target,
                    egui::vec2(max.x.min(ui.available_width()), max.y.max(150.)),
                    p,
                );
                if let Some(Entry::Failed(error)) = self.media.cache.get(&key) {
                    ui.colored_label(p.muted, error);
                    if ui.button("Retry").clicked() {
                        self.media.cache.remove(&key);
                    }
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.gallery_index > 0, egui::Button::new("← Previous"))
                        .clicked()
                    {
                        self.gallery_index = self.gallery_index.saturating_sub(1);
                    }
                    if ui
                        .add_enabled(
                            self.gallery_index + 1 < self.gallery.len(),
                            egui::Button::new("Next →"),
                        )
                        .clicked()
                    {
                        self.gallery_index += 1;
                    }
                    for (label, brave) in [("Save original", false), ("Open in Brave", true)] {
                        if ui.button(label).clicked() {
                            if !brave {
                                self.media.save_original(&target, ui.ctx());
                                continue;
                            }
                            self.app.notice = match self.media.save(&target, brave) {
                                Ok(file) => format!("Saved {}", file.display()),
                                Err(e) => e,
                            };
                        }
                    }
                });
                if let Some(Entry::Ready { data, .. }) = self.media.cache.get(&key) {
                    ui.label(
                        RichText::new(format!(
                            "{} × {} · {} KB",
                            data.dimensions.0,
                            data.dimensions.1,
                            data.bytes.len().div_ceil(1024)
                        ))
                        .small()
                        .color(p.muted),
                    );
                }
            });
        if !open {
            self.gallery.clear();
        }
    }
}
impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.reaction_picker(ctx);
        self.app.poll();
        if self.gallery.is_empty()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.app.mode = if self.app.mode == Mode::Confirm {
                self.app.confirm_body = None;
                Mode::Compose
            } else if self.app.editor.text.is_empty() {
                Mode::Browse
            } else {
                Mode::Compose
            };
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
            self.search_focus = true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::R)) {
            self.app.refresh_now();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)) {
            self.app.load_accounts();
        }
        for event in self.media.poll(ctx) {
            match event {
                Event::Search(seq, result) if seq == self.search_seq => {
                    self.search_busy = false;
                    match result {
                        Ok(v) => self.search_results = v,
                        Err(e) => self.app.notice = e,
                    }
                }
                Event::File(file) => {
                    self.file_busy = false;
                    self.app.notice = "Uploading attachment…".into();
                    self.app.upload(file);
                }
                Event::Notice(s) => {
                    self.file_busy = false;
                    self.app.notice = s;
                }
                _ => {}
            }
        }
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect::<Vec<_>>()
        });
        if self.app.active.as_ref().is_some_and(|c| c.kind == "chat")
            && let Some(path) = dropped.into_iter().next()
        {
            self.app.upload(path);
        }
        let p = self.theme.palette();
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(16, 7)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&self.app.notice).size(11.).color(p.muted))
                            .truncate(),
                    );
                    if !self.app.errors.is_empty()
                        && ui
                            .small_button(format!("{} account issues", self.app.errors.len()))
                            .clicked()
                    {
                        self.app.load_accounts();
                    }
                });
            });
        self.sidebar(ctx, p);
        self.inbox(ctx, p);
        self.thread(ctx, p);
        self.dialogs(ctx, p);
        if self.app.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let urls = ctx.output_mut(|output| {
            let mut urls = Vec::new();
            output.commands.retain(|command| {
                if let egui::OutputCommand::OpenUrl(url) = command {
                    urls.push(url.url.clone());
                    false
                } else {
                    true
                }
            });
            urls
        });
        for url in urls {
            if let Err(error) = links::open(&url) {
                self.app.notice = format!("Could not open Brave: {error}");
            }
        }
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}
// Keep loaded Slack replies beside their parent while retaining their original
// indices for reply, reaction, and edit actions. Orphan replies remain visible.
fn row_text(ui: &egui::Ui, text: &str, pos: egui::Pos2, width: f32, size: f32, color: Color32) {
    let mut colored = egui::text::LayoutJob::default();
    emoji::append(
        &mut colored,
        text,
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color,
            ..Default::default()
        },
    );
    let mut job = colored;
    job.wrap.max_width = width.max(1.);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    ui.painter().galley(pos, galley.clone(), color);
    emoji::paint(ui, &galley, pos);
}

fn message_order(messages: &[Message], group_threads: bool) -> Vec<usize> {
    if !group_threads {
        return (0..messages.len()).collect();
    }
    let ids: std::collections::BTreeSet<_> = messages.iter().map(|m| m.id.as_str()).collect();
    let mut replies: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, message) in messages.iter().enumerate() {
        if let Some(root) = message.thread_root_id.as_deref()
            && root != message.id
            && ids.contains(root)
        {
            replies.entry(root).or_default().push(index);
        }
    }
    let mut order = Vec::with_capacity(messages.len());
    for (index, message) in messages.iter().enumerate() {
        if message
            .thread_root_id
            .as_deref()
            .is_some_and(|root| root != message.id && ids.contains(root))
        {
            continue;
        }
        order.push(index);
        if let Some(children) = replies.get(message.id.as_str()) {
            order.extend(children);
        }
    }
    order
}

fn initials(name: &str) -> String {
    let result: String = name
        .split_whitespace()
        .take(2)
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .flat_map(char::to_uppercase)
        .take(2)
        .collect();
    if result.is_empty() {
        "?".into()
    } else {
        result
    }
}
pub fn run(app: App) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Chord")
            .with_app_id("dev.chord.Chord")
            .with_inner_size([1240., 820.])
            .with_min_inner_size([760., 520.]),
        renderer: eframe::Renderer::Glow,
        // Wayland already schedules presentation. Waiting for a swap interval can
        // block the event loop when a scratchpad surface is hidden.
        vsync: std::env::var_os("WAYLAND_DISPLAY").is_none(),
        ..Default::default()
    };
    eframe::run_native(
        "Chord",
        options,
        Box::new(move |cc| Ok(Box::new(Gui::new(app, &cc.egui_ctx)))),
    )
    .map_err(|e| anyhow::anyhow!("Could not open Chord: {e}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_fonts_cover_message_and_reaction_emoji() {
        let ctx = egui::Context::default();
        let _gui = Gui::new(App::new(None, true), &ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            ctx.fonts_mut(|fonts| {
                for ch in "😀😂🥹🥰👍🏽❤️🎉✅🔥👀🙏🚀🫠🫡🩷🧑💻🇺🇸"
                    .chars()
                    .filter(|ch| *ch != '\u{fe0f}')
                {
                    assert!(
                        fonts.has_glyph(&egui::FontId::proportional(18.), ch),
                        "Missing emoji glyph: {ch}"
                    );
                }
            });
        });
    }
    #[test]
    fn clicking_parent_expands_and_collapses_inline_thread() {
        let ctx = egui::Context::default();
        let mut gui = Gui::new(App::new(None, true), &ctx);
        let mut chat = gui.app.active.clone().unwrap();
        chat.source = "slack".into();
        let mut message = gui.app.messages[0].clone();
        message.reply_count = 2;
        let mut time = 0.;
        let mut frame = |gui: &mut Gui, events| {
            time += 0.4;
            let mut rect = egui::Rect::NOTHING;
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let top = ui.cursor().top();
                        gui.message(ui, &chat, &message, 0, gui.theme.palette());
                        rect = egui::Rect::from_min_max(
                            egui::pos2(ui.min_rect().left(), top),
                            egui::pos2(ui.max_rect().right(), ui.cursor().top()),
                        );
                    });
                },
            );
            rect.center()
        };
        assert!(gui.expanded_thread.is_none());
        let pos = frame(&mut gui, vec![]);
        for expanded in [true, false] {
            for pressed in [true, false] {
                frame(
                    &mut gui,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
            assert_eq!(gui.expanded_thread.is_some(), expanded);
        }
    }
    #[test]
    fn inline_threads_keep_replies_with_parent_and_preserve_action_indices() {
        let message = |id: &str, root: Option<&str>| Message {
            id: id.into(),
            thread_root_id: root.map(str::to_owned),
            ..Default::default()
        };
        let messages = vec![
            message("parent", None),
            message("other", None),
            message("reply", Some("parent")),
            message("orphan", Some("unloaded")),
            message("second-reply", Some("parent")),
        ];
        assert_eq!(message_order(&messages, true), vec![0, 2, 4, 1, 3]);
        assert_eq!(message_order(&messages, false), vec![0, 1, 2, 3, 4]);
    }
    #[test]
    fn initials_handle_unicode_and_empty_names() {
        assert_eq!(initials("Alex Chen"), "AC");
        assert_eq!(initials("Élodie"), "É");
        assert_eq!(initials(""), "?");
    }
    #[test]
    fn reaction_gestures_target_the_clicked_message_and_respect_capabilities() {
        for (button, clicks, enabled) in [
            (egui::PointerButton::Secondary, 1, true),
            (egui::PointerButton::Primary, 2, true),
            (egui::PointerButton::Secondary, 1, false),
        ] {
            let ctx = egui::Context::default();
            let mut gui = Gui::new(App::new(None, true), &ctx);
            let mut chat = gui.app.active.clone().unwrap();
            chat.can_react = enabled;
            let message = gui.app.messages[0].clone();
            let mut time = 0.;
            let mut rect = egui::Rect::NOTHING;
            let mut frame = |gui: &mut Gui, events| {
                time += 0.08;
                let _ = ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let top = ui.cursor().top();
                            gui.message(ui, &chat, &message, 0, gui.theme.palette());
                            rect = egui::Rect::from_min_max(
                                egui::pos2(ui.min_rect().left(), top),
                                egui::pos2(ui.max_rect().right(), ui.cursor().top()),
                            );
                        });
                    },
                );
                rect.center()
            };
            let pos = frame(&mut gui, vec![]);
            for _ in 0..clicks {
                frame(
                    &mut gui,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button,
                            pressed: true,
                            modifiers: Default::default(),
                        },
                    ],
                );
                frame(
                    &mut gui,
                    vec![egui::Event::PointerButton {
                        pos,
                        button,
                        pressed: false,
                        modifiers: Default::default(),
                    }],
                );
            }
            assert_eq!(
                gui.reaction_target.is_some(),
                enabled,
                "button={button:?} clicks={clicks} pos={pos:?} rect={rect:?}"
            );
            if let Some((target_chat, target_message)) = gui.reaction_target {
                assert_eq!(target_chat.id, chat.id);
                assert_eq!(target_message.id, message.id);
            }
        }
    }
    #[test]
    fn conversation_avatar_children_do_not_overlap_following_rows() {
        for contacts in [vec![], vec!["one".into(), "two".into(), "three".into()]] {
            let ctx = egui::Context::default();
            let mut gui = Gui::new(App::new(None, true), &ctx);
            let mut chat = gui.app.active.clone().unwrap();
            chat.direct = false;
            chat.contact_ids = contacts;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let p = gui.theme.palette();
                    let first = gui.conversation_row(ui, &chat, p).rect;
                    let second = gui.conversation_row(ui, &chat, p).rect;
                    assert!(
                        second.top() >= first.bottom(),
                        "{first:?} overlaps {second:?}"
                    );
                });
            });
        }
    }
    #[test]
    fn native_layout_renders_at_normal_and_compact_sizes() {
        for size in [[1240., 820.], [800., 600.]] {
            let ctx = egui::Context::default();
            let mut gui = Gui::new(App::new(None, true), &ctx);
            gui.app.active.as_mut().unwrap().source = "slack".into();
            gui.app.editor.set("Draft 🎉 👩‍💻 🇺🇸 👍🏽".into());
            gui.app.messages[1].thread_root_id = Some(gui.app.messages[0].id.clone());
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::from(size),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    let p = gui.theme.palette();
                    gui.sidebar(ctx, p);
                    gui.inbox(ctx, p);
                    gui.thread(ctx, p);
                },
            );
            assert!(!output.shapes.is_empty());
        }
    }
}
