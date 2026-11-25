mod emoji;
mod icons;
pub(crate) mod links;
mod media;
mod theme;

use crate::{
    api::Api,
    app::{App, Mode, keeps_failure, reaction_is_users, user_has_reaction},
    images::Target,
    model::{Conversation, Message, day_label, one_line, source_label, time_label},
};
use eframe::egui::{self, Color32, RichText, Vec2};
use media::{DiscordEmoji, Entry, Event, Media, MentionMember, Request, RoomEmoji};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::Duration,
};
use theme::{ColorScheme, DisplayMode, FontStyle, Palette, Theme};

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum AppearanceSmokeTest {
    Write,
    Check,
}

struct AppearanceSmoke {
    mode: AppearanceSmokeTest,
    rendered: bool,
    complete: std::rc::Rc<std::cell::Cell<bool>>,
}

pub struct Gui {
    pub app: App,
    media: Media,
    theme: Theme,
    icon_style: icons::IconStyle,
    search: String,
    gallery: Vec<Target>,
    gallery_index: usize,
    file_busy: bool,
    profile_name: String,
    profile_id: String,
    settings_path: PathBuf,
    appearance_smoke: Option<AppearanceSmoke>,
    scroll_to: Option<String>,
    unread_marker: Option<String>,
    search_focus: bool,
    message_search: bool,
    reaction_target: Option<ReactionPicker>,
    discord_emoji: BTreeMap<String, Option<Result<Vec<DiscordEmoji>, String>>>,
    discord_emoji_requested: BTreeMap<String, std::time::Instant>,
    room_custom_emoji: BTreeMap<String, Vec<RoomEmoji>>,
    emoji_usage: BTreeMap<String, BTreeSet<String>>,
    composer_completion_open: bool,
    mention_members: BTreeMap<String, (std::time::Instant, Vec<MentionMember>)>,
    reaction_detail: Option<ReactionDetail>,
    expanded_thread: Option<(String, String)>,
    search_busy: bool,
    search_seq: u64,
    search_results: Value,
    logo: Option<egui::TextureHandle>,
}
impl Gui {
    pub fn new(app: App, ctx: &egui::Context) -> Self {
        Self::with_settings_path(app, ctx, settings_path())
    }

    fn with_settings_path(mut app: App, ctx: &egui::Context, settings_path: PathBuf) -> Self {
        app.native_ui = true;
        let settings: Value = std::fs::read(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok())
            .unwrap_or(Value::Null);
        app.pins = serde_json::from_value(settings["pins"].clone()).unwrap_or_default();
        app.hidden_accounts =
            serde_json::from_value(settings["hiddenAccounts"].clone()).unwrap_or_default();
        app.unimportant_discord_servers =
            serde_json::from_value(settings["unimportantDiscordServers"].clone())
                .unwrap_or_default();
        if !app.unimportant_discord_servers.is_empty() {
            app.refresh(false);
        }
        let theme = Theme::from_settings(&settings);
        theme.apply(ctx);
        Self::apply_fonts(ctx, theme.font);
        Self {
            app,
            media: Media::default(),
            theme,
            icon_style: icons::IconStyle::from_setting(settings["iconStyle"].as_str()),
            search: String::new(),
            gallery: vec![],
            gallery_index: 0,
            file_busy: false,
            profile_name: String::new(),
            profile_id: String::new(),
            settings_path,
            appearance_smoke: None,
            scroll_to: None,
            unread_marker: None,
            search_focus: false,
            message_search: false,
            reaction_target: None,
            discord_emoji: BTreeMap::new(),
            discord_emoji_requested: BTreeMap::new(),
            room_custom_emoji: BTreeMap::new(),
            emoji_usage: serde_json::from_value(settings["reactionUsage"].clone())
                .unwrap_or_default(),
            composer_completion_open: false,
            mention_members: BTreeMap::new(),
            reaction_detail: None,
            expanded_thread: None,
            search_busy: false,
            search_seq: 0,
            search_results: Value::Null,
            logo: None,
        }
    }
    fn apply_fonts(ctx: &egui::Context, font: FontStyle) {
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
        if font == FontStyle::Terminal {
            let monospace = fonts.families[&egui::FontFamily::Monospace].clone();
            fonts
                .families
                .insert(egui::FontFamily::Proportional, monospace);
        } else if cfg!(target_os = "macos") {
            for (name, path, family) in [
                (
                    "sf",
                    "/System/Library/Fonts/SFNS.ttf",
                    egui::FontFamily::Proportional,
                ),
                (
                    "sf-mono",
                    "/System/Library/Fonts/SFNSMono.ttf",
                    egui::FontFamily::Monospace,
                ),
            ] {
                if let Ok(bytes) = std::fs::read(path) {
                    fonts.font_data.insert(
                        name.into(),
                        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                    );
                    fonts
                        .families
                        .entry(family)
                        .or_default()
                        .insert(0, name.into());
                }
            }
        }
        ctx.set_fonts(fonts);
    }
    fn show_logo(&mut self, ui: &mut egui::Ui, size: f32, fallback: RichText) {
        if self.logo.is_none()
            && let Ok(icon) = image::load_from_memory(include_bytes!("../assets/macos/icon.png"))
        {
            let side = icon.width() * 824 / 1024;
            let margin = (icon.width() - side) / 2;
            let icon = icon
                .crop_imm(margin, margin, side, side)
                .thumbnail(192, 192)
                .into_rgba8();
            self.logo = Some(ui.ctx().load_texture(
                "chord-logo",
                egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width() as usize, icon.height() as usize],
                    icon.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            ));
        }
        if let Some(logo) = &self.logo {
            ui.add(egui::Image::new(logo).fit_to_exact_size(Vec2::splat(size)));
        } else {
            ui.label(fallback);
        }
    }
    fn key(&mut self, code: crossterm::event::KeyCode) {
        self.app.key(crossterm::event::KeyEvent::new(
            code,
            crossterm::event::KeyModifiers::NONE,
        ));
    }
    fn save_theme(&mut self, ctx: &egui::Context, previous: Theme) {
        if previous.font != self.theme.font {
            Self::apply_fonts(ctx, self.theme.font);
        }
        self.theme.apply(ctx);
        self.save_settings();
    }
    fn save_settings(&mut self) {
        let value = json!({
            "appearance": self.theme,
            "reactionUsage": self.emoji_usage,
            "iconStyle": self.icon_style.setting(),
            "pins": self.app.pins,
            "hiddenAccounts": self.app.hidden_accounts,
            "unimportantDiscordServers": self.app.unimportant_discord_servers,
        });
        let result = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            if let Some(dir) = self.settings_path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let temporary =
                self.settings_path
                    .with_extension(format!("{}.{}.tmp", std::process::id(), nonce));
            let mut file = opts.open(&temporary)?;
            file.write_all(value.to_string().as_bytes())?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(temporary, &self.settings_path)
        })();
        if let Err(e) = result {
            self.app.notice = format!("Could not save settings: {e}");
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
        self.reaction_detail = None;
        self.app.mode = Mode::Browse;
        self.app.open(chat);
        self.scroll_to = None;
        self.unread_marker = None;
    }
    fn sidebar(&mut self, ctx: &egui::Context, p: Palette) {
        egui::SidePanel::left("navigation")
            .exact_width(54.)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(6, 12)),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    self.show_logo(
                        ui,
                        30.,
                        RichText::new("Chord").size(30.).color(p.accent).strong(),
                    );
                    ui.add_space(10.);
                    egui::containers::menu::MenuButton::new("⚙")
                        .config(
                            egui::containers::menu::MenuConfig::new()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| self.settings_menu(ui, p))
                        .0
                        .on_hover_text("Settings");
                });
            });
    }

    fn toggle_account(&mut self, key: &str, visible: bool) {
        if visible {
            self.app.hidden_accounts.remove(key);
        } else {
            self.app.hidden_accounts.insert(key.to_owned());
        }
        self.app.selection = 0;
        self.save_settings();
    }

    fn account_filter(&mut self, ui: &mut egui::Ui, compact: bool) {
        let label = if compact { "≡" } else { "Accounts" };
        let id = ui.make_persistent_id("account-filter");
        let button = ui.small_button(label).on_hover_text("Filter accounts");
        if button.clicked() && !egui::Popup::is_id_open(ui.ctx(), id) {
            self.app.refresh_discord_servers();
        }
        egui::Popup::menu(&button)
            .id(id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.label("Show accounts");
                let configured = self.app.connections["items"].as_array();
                let accounts: Vec<_> = crate::model::SOURCES
                    .iter()
                    .skip(1)
                    .filter(|(key, _, _)| {
                        configured.is_none_or(|items| items.iter().any(|row| row["id"] == *key))
                            || self
                                .app
                                .conversations
                                .iter()
                                .any(|chat| chat.source == *key)
                    })
                    .collect();
                let workspaces: BTreeMap<_, _> = self
                    .app
                    .conversations
                    .iter()
                    .filter(|chat| chat.source == "slack" && chat.account_key.starts_with("slack:"))
                    .filter_map(|chat| {
                        chat.account_name
                            .as_ref()
                            .map(|name| (chat.account_key.clone(), name.clone()))
                    })
                    .collect();
                for &(key, label, _) in accounts {
                    let mut visible = !self.app.hidden_accounts.contains(key);
                    if ui.checkbox(&mut visible, label).changed() {
                        self.toggle_account(key, visible);
                    }
                    if key == "slack" && workspaces.len() > 1 {
                        ui.indent("slack-workspaces", |ui| {
                            ui.add_enabled_ui(visible, |ui| {
                                for (key, name) in &workspaces {
                                    let mut visible = !self.app.hidden_accounts.contains(key);
                                    if ui.checkbox(&mut visible, name).changed() {
                                        self.toggle_account(key, visible);
                                    }
                                }
                            });
                        });
                    }
                }
                ui.separator();
                if self.app.source != 0 && ui.button("All conversations").clicked() {
                    self.switch_source(0);
                    ui.close();
                }
                if ui.button("Browse Discord servers…").clicked() {
                    self.app.open_servers();
                    ui.close();
                }
                ui.menu_button("Discord server importance", |ui| {
                    ui.label("Mark servers as unimportant");
                    if self.app.servers.is_empty() {
                        ui.weak("No Discord servers loaded");
                    }
                    let mut changed = false;
                    egui::ScrollArea::vertical()
                        .max_height(300.)
                        .show(ui, |ui| {
                            for server in &self.app.servers {
                                let (Some(id), Some(name)) =
                                    (server["id"].as_str(), server["name"].as_str())
                                else {
                                    continue;
                                };
                                let mut unimportant =
                                    self.app.unimportant_discord_servers.contains(id);
                                if ui.checkbox(&mut unimportant, name).changed() {
                                    if unimportant {
                                        self.app.unimportant_discord_servers.insert(id.into());
                                    } else {
                                        self.app.unimportant_discord_servers.remove(id);
                                    }
                                    changed = true;
                                }
                            }
                        });
                    if changed {
                        self.app.selection = 0;
                        self.save_settings();
                        self.app.refresh(false);
                    }
                });
            });
    }

    fn settings_menu(&mut self, ui: &mut egui::Ui, p: Palette) {
        ui.set_width(278.0);
        ui.spacing_mut().item_spacing.y = 5.0;
        ui.label(RichText::new("Settings").size(19.0).strong());
        ui.separator();
        ui.label(
            RichText::new("Appearance")
                .size(11.0)
                .strong()
                .color(p.muted),
        );

        let previous = self.theme;
        let old_icons = self.icon_style;
        let mode = self
            .theme
            .mode
            .unwrap_or_else(|| DisplayMode::current(ui.ctx().theme()));
        ui.label("Display mode");
        if let Some(mode) = settings_choices(
            ui,
            p,
            mode,
            [(DisplayMode::Light, "Light"), (DisplayMode::Dark, "Dark")],
        ) {
            self.theme.mode = Some(mode);
        }

        ui.add_space(3.0);
        let id = ui.make_persistent_id("appearance-color-scheme");
        let mut schemes =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        let arrow = if schemes.is_open() { "▴" } else { "▾" };
        let value = format!("{}  {arrow}", self.theme.scheme.label());
        let scheme_button = egui::Button::new("Color scheme")
            .right_text(RichText::new(value).color(p.muted))
            .corner_radius(7.0);
        if ui
            .add_sized([ui.available_width(), 30.0], scheme_button)
            .clicked()
        {
            schemes.toggle(ui);
        }
        schemes.show_body_unindented(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for scheme in ColorScheme::OPTIONS {
                let selected = self.theme.scheme == scheme;
                let option = egui::Button::selectable(selected, scheme.label())
                    .right_text(if selected { "✓" } else { "" })
                    .corner_radius(6.0);
                if ui.add_sized([ui.available_width(), 24.0], option).clicked() {
                    self.theme.scheme = scheme;
                }
            }
        });

        ui.add_space(3.0);
        ui.label("Font style");
        if let Some(font) = settings_choices(
            ui,
            p,
            self.theme.font,
            [
                (FontStyle::Terminal, "Terminal"),
                (FontStyle::MacOS, "macOS"),
            ],
        ) {
            self.theme.font = font;
        }

        ui.add_space(3.0);
        ui.label("Service icons");
        if let Some(style) = settings_choices(
            ui,
            p,
            self.icon_style,
            icons::IconStyle::OPTIONS.map(|style| (style, style.label())),
        ) {
            self.icon_style = style;
        }

        if self.theme != previous {
            self.save_theme(ui.ctx(), previous);
        } else if self.icon_style != old_icons {
            self.save_settings();
        }

        ui.separator();
        if ui
            .add_sized(
                [ui.available_width(), 30.],
                egui::Button::new("Contacts & profiles")
                    .right_text("›")
                    .frame_when_inactive(false),
            )
            .clicked()
        {
            self.app.open_contacts();
            ui.close();
        }
        if ui
            .add_sized(
                [ui.available_width(), 30.0],
                egui::Button::new("Manage accounts")
                    .right_text("›")
                    .frame_when_inactive(false),
            )
            .clicked()
        {
            self.app.load_accounts();
            ui.close();
        }
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
    fn inbox_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Inbox");
            if self.app.loading {
                ui.spinner();
            }
        });
        ui.add_space(3.0);
        let width = ui.available_width();
        let compact = width < 320.;
        ui.allocate_ui_with_layout(
            egui::vec2(width, ui.spacing().interact_size.y),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = if compact { 3. } else { 5. };
                ui.spacing_mut().button_padding.x = 4.;
                if ui.small_button("↻").on_hover_text("Refresh").clicked() {
                    self.app.refresh_now();
                }
                self.account_filter(ui, compact);
                if !self.message_search {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.spacing_mut().icon_width = if compact { 10. } else { 12. };
                        ui.spacing_mut().icon_width_inner = if compact { 7. } else { 8. };
                        let size = if width < 260. { 10. } else { 12. };
                        let show = ui
                            .add(egui::Checkbox::new(
                                &mut self.app.show_unimportant,
                                RichText::new("Show unimportant").size(size),
                            ))
                            .changed();
                        let unread = ui
                            .add(egui::Checkbox::new(
                                &mut self.app.unread,
                                RichText::new("Unread only").size(size),
                            ))
                            .changed();
                        if show || unread {
                            self.app.selection = 0;
                            self.app.refresh(false);
                        }
                    });
                }
            },
        );
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
                self.inbox_header(ui);
                ui.add_space(10.);
                ui.horizontal(|ui| {
                    let compact = ui.available_width() < 295.;
                    let size = if compact { 12. } else { 14. };
                    if ui.selectable_label(!self.message_search, RichText::new("Conversations").size(size)).clicked() { self.message_search = false; }
                    if ui.selectable_label(self.message_search, RichText::new("Message text").size(size)).clicked() { self.message_search = true; self.run_search(ctx); }
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
                    let hits: Vec<_> = self.search_results["items"].as_array().into_iter().flatten()
                        .filter_map(|hit| {
                            let chat = serde_json::from_value::<Conversation>(hit["conversation"].clone()).ok()?;
                            self.app.account_visible(&chat).then_some((hit.clone(), chat))
                        }).collect();
                    egui::ScrollArea::vertical().id_salt("search-results").show(ui, |ui| {
                        if hits.is_empty() && !self.search_busy { ui.label("No results. Enter text and press Search."); }
                        for (hit, chat) in hits {
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
                                } else if self.app.unread {
                                    "No unread conversations in this view."
                                } else {
                                    "Nothing here yet. Try another account or refresh."
                                })
                                .color(p.muted),
                            );
                        }
                        let pinned = self.app.pinned_count();
                        let mut rows = Vec::with_capacity(chats.len());
                        let mut divider = None;
                        let mut drag = None;
                        for (chat_index, chat) in chats.iter().enumerate() {
                            if pinned > 0 && chat_index == 0 {
                                ui.label(RichText::new("Pinned").small().color(p.muted));
                            }
                            if pinned > 0 && chat_index == pinned {
                                divider = Some(ui.separator().rect.center().y);
                            }
                            let response = self.conversation_row(ui, chat, p);
                            rows.push(response.rect);
                            if response.dragged() || response.drag_stopped() {
                                drag = ui
                                    .ctx()
                                    .input(|i| i.pointer.latest_pos())
                                    .map(|pos| (chat.id.clone(), pos, response.drag_stopped()));
                            }
                            let pointer_on_row = response.interact_pointer_pos().is_some_and(|pos| {
                                ui.ctx().layer_id_at(pos) == Some(response.layer_id)
                            });
                            if response.clicked() && pointer_on_row {
                                self.app.selection = chat_index;
                                self.choose_chat(chat.clone());
                                ctx.request_repaint();
                            }
                        }
                        if pinned > 0 && pinned == chats.len() {
                            divider = Some(ui.separator().rect.center().y);
                        }
                        if let Some((id, pos, dropped)) = drag {
                            self.drag_chat(ui, &chats, &rows, divider, pinned, &id, pos, dropped, p);
                        }
                        if self.app.has_more_inbox()
                            && ui.add_enabled(!self.app.loading, egui::Button::new("Load more conversations")).clicked()
                        {
                            self.app.refresh(true);
                        }
                    });
            });
    }
    // Dropping above the pinned divider pins the chat at that spot; below it unpins.
    #[allow(clippy::too_many_arguments)]
    fn drag_chat(
        &mut self,
        ui: &mut egui::Ui,
        chats: &[Conversation],
        rows: &[egui::Rect],
        divider: Option<f32>,
        pinned: usize,
        id: &str,
        pos: egui::Pos2,
        dropped: bool,
        p: Palette,
    ) {
        let Some(first) = rows.first() else {
            return;
        };
        let gap = rows.iter().filter(|r| r.center().y < pos.y).count();
        let pin = gap < pinned || (gap == pinned && divider.is_none_or(|y| pos.y < y));
        let position = chats[..gap]
            .iter()
            .filter(|c| c.id != id && self.app.is_pinned(&c.id))
            .count();
        let was_pinned = self.app.is_pinned(id);
        if dropped {
            if !ui.clip_rect().contains(pos) {
                return;
            }
            if pin {
                self.pin_chat(id, position);
            } else if was_pinned {
                self.app.move_pin(id, None);
                self.save_settings();
                self.app.refresh(false);
            }
            return;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let clip = ui.clip_rect();
        if pos.y < clip.top() + 40. {
            ui.scroll_with_delta(egui::vec2(0., 10.));
        } else if pos.y > clip.bottom() - 40. {
            ui.scroll_with_delta(egui::vec2(0., -10.));
        }
        if pin {
            let y = if gap == 0 {
                first.top()
            } else {
                rows[gap - 1].bottom()
            };
            ui.painter()
                .hline(first.x_range(), y, egui::Stroke::new(2.5_f32, p.accent));
        }
        let title = chats
            .iter()
            .find(|c| c.id == id)
            .map_or("", |c| c.title.as_str());
        let label = match (pin, was_pinned) {
            (true, _) => format!("Pin {title}"),
            (false, true) => format!("Unpin {title}"),
            (false, false) => title.to_owned(),
        };
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("chat-drag"),
        ));
        let galley = painter.layout_no_wrap(label, egui::FontId::proportional(14.), p.text);
        let rect = egui::Rect::from_min_size(pos + egui::vec2(14., -10.), galley.size())
            .expand2(egui::vec2(10., 6.));
        painter.rect_filled(rect, 6., p.raised);
        painter.galley(rect.min + egui::vec2(10., 6.), galley, p.text);
        ui.ctx().request_repaint();
    }
    fn conversation_row(
        &mut self,
        ui: &mut egui::Ui,
        chat: &Conversation,
        p: Palette,
    ) -> egui::Response {
        ui.push_id(&chat.id, |ui| self.conversation_row_inner(ui, chat, p))
            .inner
    }
    fn conversation_row_inner(
        &mut self,
        ui: &mut egui::Ui,
        chat: &Conversation,
        p: Palette,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 90.),
            egui::Sense::click_and_drag(),
        );
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let active = self.app.active.as_ref().is_some_and(|c| c.id == chat.id);
        let account_key = if chat.account_key.is_empty() {
            chat.source.as_str()
        } else {
            chat.account_key.as_str()
        };
        let color = theme::account_color(account_key);
        if active || response.hovered() {
            ui.painter().rect_filled(
                rect.shrink2(egui::vec2(0., 3.)),
                8.,
                if active {
                    p.select
                } else {
                    theme::tint(p.panel, p.raised, 0.45)
                },
            );
        }
        // Room for the unread dot on the left. Avatars stay put either way.
        let avatar_rect =
            egui::Rect::from_min_size(rect.min + egui::vec2(24., 23.), Vec2::splat(44.));
        if chat.unread {
            ui.painter().circle_filled(
                egui::pos2(rect.left() + 11., avatar_rect.center().y),
                7.,
                p.blue,
            );
        }
        let group =
            !chat.direct && matches!(chat.source.as_str(), "slack" | "discord" | "whatsapp");
        let members: Vec<(&str, &str)> = if !chat.avatar_contacts.is_empty() {
            chat.avatar_contacts
                .iter()
                .filter_map(|person| {
                    Some((
                        person["id"].as_str()?,
                        person["name"].as_str().unwrap_or("?"),
                    ))
                })
                .take(3)
                .collect()
        } else {
            chat.contact_ids
                .iter()
                .take(3)
                .map(|id| (id.as_str(), "?"))
                .collect()
        };
        let multi = group && !members.is_empty();
        // The row already owns its layout space. Avatar children must not
        // advance the parent cursor back into that reserved row.
        {
            let mut avatar_ui = ui.new_child(egui::UiBuilder::new().max_rect(avatar_rect));
            let ui = &mut avatar_ui;
            if !multi {
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
                for (index, (id, name)) in members.iter().enumerate() {
                    let offset = [
                        egui::vec2(0., 0.),
                        egui::vec2(20., 0.),
                        egui::vec2(10., 20.),
                    ][index];
                    let cell =
                        egui::Rect::from_min_size(avatar_rect.min + offset, Vec2::splat(25.));
                    let mut cell_ui = ui.new_child(egui::UiBuilder::new().max_rect(cell));
                    ui.painter().circle_filled(cell.center(), 14.5, p.panel);
                    self.avatar(&mut cell_ui, Some(id), name, 25., p);
                }
            }
        }
        // Account color rings the profile image. A group rings each face.
        // Sit inside the photo so the ring does not crowd the unread dot.
        let account_ring = egui::Stroke::new(2.0_f32, color);
        if !multi {
            ui.painter()
                .circle_stroke(avatar_rect.center(), 19., account_ring);
        } else {
            for (index, _) in members.iter().enumerate() {
                let offset = [
                    egui::vec2(0., 0.),
                    egui::vec2(20., 0.),
                    egui::vec2(10., 20.),
                ][index];
                let cell = egui::Rect::from_min_size(avatar_rect.min + offset, Vec2::splat(25.));
                ui.painter()
                    .circle_stroke(cell.center(), 10.5, account_ring);
            }
        }
        let badge = egui::Rect::from_center_size(
            avatar_rect.right_bottom() - egui::vec2(2., 2.),
            Vec2::splat(20.),
        );
        // A plain disk with a hairline edge, so the service's own colors stay true.
        ui.painter().circle(
            badge.center(),
            11.,
            p.panel,
            egui::Stroke::new(1.0_f32, p.line),
        );
        let source = crate::model::SOURCES
            .iter()
            .position(|(id, _, _)| *id == chat.source)
            .unwrap_or(0);
        icons::service(
            ui.painter(),
            badge.shrink(2.5),
            source,
            p.text,
            self.icon_style,
        );
        let text_left = avatar_rect.right() + 16.;
        let right = rect.right() - 9.;
        let mut stamp = time_label(&chat.timestamp);
        let now_ms = chrono::Utc::now().timestamp_millis();
        if chat.mute_until.is_some_and(|until| until > now_ms) {
            stamp = format!("Muted · {stamp}");
        } else if chat.local_priority == "unimportant" {
            stamp = format!("Unimportant · {stamp}");
        }
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
            .filter(|name| !name.is_empty())
            .or(chat.account_name.as_deref().filter(|name| !name.is_empty()))
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
        // Pinned chats show a pin under the time, where the preview would otherwise end.
        let pinned = self.app.is_pinned(&chat.id);
        if pinned {
            icons::pin(
                ui.painter(),
                egui::pos2(right - 6., rect.top() + 59.),
                13.,
                p.muted,
            );
        }
        row_text(
            ui,
            &preview,
            egui::pos2(text_left, rect.top() + 50.),
            right - text_left - if pinned { 26. } else { 8. },
            14.,
            p.muted,
        );
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
        response.context_menu(|ui| self.channel_rules(ui, chat));
        // The hover tip is a higher layer than the menu and covers its items.
        if response.context_menu_opened() || response.secondary_clicked() {
            response
        } else {
            response.on_hover_text(format!("{}\n{}", chat.title, context))
        }
    }
    fn pin_chat(&mut self, id: &str, position: usize) {
        self.app.move_pin(id, Some(position));
        self.save_settings();
        self.app.refresh(false);
    }
    fn channel_rules(&mut self, ui: &mut egui::Ui, chat: &Conversation) {
        let id = chat.id.clone();
        let pinned = self.app.is_pinned(&id);
        if ui.button(if pinned { "Unpin" } else { "Pin" }).clicked() {
            if pinned {
                self.app.move_pin(&id, None);
                self.save_settings();
                self.app.refresh(false);
            } else {
                let position = self.app.pinned_count();
                self.pin_chat(&id, position);
            }
            ui.close();
        }
        ui.separator();
        let unimportant = chat.local_priority == "unimportant";
        if ui
            .button(if unimportant {
                "Restore to Inbox"
            } else {
                "Mark as unimportant"
            })
            .clicked()
        {
            self.app.set_unimportant(&id, !unimportant);
            ui.close();
        }
        ui.separator();
        for (label, minutes) in [
            ("Mute for 1 hour", 60),
            ("Mute for 4 hours", 240),
            ("Mute for 24 hours", 1440),
            ("Mute for 3 days", 4320),
            ("Mute for 7 days", 10080),
        ] {
            if ui.button(label).clicked() {
                self.app.apply_channel_rule(&id, None, Some(Some(minutes)));
                self.app.operation(
                    "Channel muted",
                    "/api/preferences/conversation".into(),
                    json!({"conversationID":id,"muteMinutes":minutes}),
                );
                ui.close();
            }
        }
        if chat
            .mute_until
            .is_some_and(|until| until > chrono::Utc::now().timestamp_millis())
            && ui.button("Unmute").clicked()
        {
            self.app.apply_channel_rule(&id, None, Some(None));
            self.app.operation(
                "Channel unmuted",
                "/api/preferences/conversation".into(),
                json!({"conversationID":id,"muteMinutes":null}),
            );
            ui.close();
        }
    }
    fn avatar(
        &mut self,
        ui: &mut egui::Ui,
        contact: Option<&str>,
        name: &str,
        size: f32,
        _p: Palette,
    ) -> egui::Response {
        self.avatar_image(
            ui,
            contact.map(|id| Request::Avatar(id.to_owned())),
            name,
            size,
        )
    }

    fn avatar_image(
        &mut self,
        ui: &mut egui::Ui,
        request: Option<Request>,
        name: &str,
        size: f32,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
        let initials = initials(name);
        if initials.is_empty() {
            // A phone number or blank name has no initials; show a contact silhouette.
            icons::person(ui.painter(), rect.center(), size / 2.);
        } else {
            ui.painter()
                .circle_filled(rect.center(), size / 2., theme::avatar_color(name));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                initials,
                egui::FontId::proportional(size * 0.34),
                Color32::WHITE,
            );
        }
        if ui.is_rect_visible(rect)
            && let Some(request) = request
        {
            let key = request.key();
            self.media.request(request, self.app.api.clone(), ui.ctx());
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
                        self.show_logo(ui, 64., RichText::new("Chord").size(58.).color(p.accent));
                        ui.heading("Your conversations");
                        ui.label(
                            RichText::new("Slack, Discord, WhatsApp, Messages, and email")
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
                            self.channel_rules(ui, &chat);
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
                if let Some(id) = self.app.first_unread.take() {
                    if let Some(root) = self
                        .app
                        .messages
                        .iter()
                        .find(|m| m.id == id)
                        .and_then(|m| m.thread_root_id.clone())
                    {
                        self.expanded_thread = Some((chat.id.clone(), root));
                    }
                    self.unread_marker = Some(id.clone());
                    self.scroll_to = Some(id);
                }
                egui::ScrollArea::vertical()
                    .id_salt(("thread", &chat.id))
                    .auto_shrink([false, false])
                    .stick_to_bottom(self.scroll_to.is_none())
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
                            if self.unread_marker.as_ref() == Some(&message.id) {
                                let divider = ui
                                    .horizontal(|ui| {
                                        ui.separator();
                                        ui.label(
                                            RichText::new("New messages").small().color(p.accent),
                                        );
                                        ui.separator();
                                    })
                                    .response;
                                if self.scroll_to.as_ref() == Some(&message.id) {
                                    divider.scroll_to_me(Some(egui::Align::TOP));
                                    self.scroll_to = None;
                                }
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
        let synced = !m.is_local_outgoing();
        let has_thread = synced
            && chat.source == "slack"
            && chat.thread_root_id.is_none()
            && m.thread_root_id.is_none()
            && (m.reply_count > 0
                || self
                    .app
                    .messages
                    .iter()
                    .any(|reply| reply.thread_root_id.as_ref() == Some(&m.id)));
        let actions_id = ui.id().with(("message-actions", &m.id));
        let more_id = actions_id.with("more");
        let can_react = synced && chat.can_react;
        let can_reply = synced && chat.can_reply;
        let can_edit = synced && m.is_sender && chat.kind == "chat";
        let show_quick_reactions = can_react && ui.available_width() >= 350.;
        let buttons = 1
            + usize::from(can_react)
            + usize::from(can_reply)
            + usize::from(can_edit)
            + 3 * usize::from(show_quick_reactions);
        let toolbar_width = 2. * (ACTION_PADDING + 1.)
            + buttons as f32 * ACTION_SIZE
            + (buttons - 1) as f32 * ACTION_GAP
            + if show_quick_reactions {
                ACTION_GAP + 1.
            } else {
                0.
            };
        let background = ui.painter().add(egui::Shape::Noop);
        let mut thread_clicked = false;
        let reaction_clicked = std::cell::Cell::new(false);
        let response = ui
            .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
                egui::Frame::new()
                    .corner_radius(12)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            let image = if m.is_sender
                                && matches!(chat.source.as_str(), "discord" | "slack" | "whatsapp")
                            {
                                Some(Request::SelfAvatar(chat.id.clone()))
                            } else {
                                m.sender_contact_id
                                    .as_ref()
                                    .map(|id| Request::Avatar(id.clone()))
                            };
                            if self.avatar_image(ui, image, &m.sender_name, 34.).clicked()
                                && let Some(id) = &m.sender_contact_id
                            {
                                self.app.open_contact(json!({"id":id}));
                            }
                            ui.vertical(|ui| {
                                ui.set_width(ui.available_width());
                                let mut metadata = time_label(&m.timestamp);
                                if m.historical_version {
                                    if !metadata.is_empty() {
                                        metadata.push_str(" · ");
                                    }
                                    metadata.push_str("Saved version");
                                }
                                let metadata = ui.painter().layout_no_wrap(
                                    metadata,
                                    egui::FontId::proportional(11.),
                                    p.muted,
                                );
                                let header_width = ui.available_width() - toolbar_width;
                                let stack_timestamp =
                                    !metadata.is_empty() && header_width < metadata.size().x + 72.;
                                let header = |ui: &mut egui::Ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(8., 0.);
                                    let name_width = if stack_timestamp || metadata.is_empty() {
                                        header_width
                                    } else {
                                        header_width
                                            - metadata.size().x
                                            - ui.spacing().item_spacing.x
                                    };
                                    let name = row_text_galley(
                                        ui,
                                        &m.sender_name,
                                        name_width,
                                        14.,
                                        if m.is_sender {
                                            p.accent
                                        } else {
                                            theme::avatar_color(&m.sender_name)
                                        },
                                    );
                                    let (name_rect, name_response) = ui.allocate_exact_size(
                                        egui::vec2(name.size().x, 22.),
                                        egui::Sense::click(),
                                    );
                                    let name_pos = egui::pos2(
                                        name_rect.left(),
                                        name_rect.center().y - name.size().y / 2.,
                                    );
                                    ui.painter().galley(name_pos, name.clone(), p.text);
                                    emoji::paint(ui, &name, name_pos);
                                    if name_response.clicked()
                                        && let Some(id) = &m.sender_contact_id
                                    {
                                        self.app.open_contact(json!({"id":id}));
                                    }
                                    if !metadata.is_empty() {
                                        ui.add(egui::Label::new(metadata).truncate());
                                    }
                                };
                                if stack_timestamp {
                                    ui.vertical(header);
                                } else {
                                    ui.horizontal(header);
                                }
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
                                    for emote in &m.emotes {
                                        if emote.mxc.starts_with("mxc://") {
                                            self.media.request(
                                                Request::Emoji(emote.mxc.clone()),
                                                self.app.api.clone(),
                                                ui.ctx(),
                                            );
                                        }
                                    }
                                    thread_clicked |=
                                        links::rich_text(ui, &m.text, &m.emotes, |mxc| {
                                            self.media.texture(&format!("emoji:{mxc}"))
                                        });
                                }
                                if let Some(error) = &m.send_error {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(format!("Not sent · {error}"))
                                                .small()
                                                .color(p.accent),
                                        );
                                        if ui.small_button("Retry").clicked() {
                                            thread_clicked = true;
                                            self.app.retry_outgoing(&m.id);
                                        }
                                        if ui.small_button("Dismiss").clicked() {
                                            thread_clicked = true;
                                            self.app.dismiss_outgoing(&m.id);
                                        }
                                    });
                                } else if let Some(status) = &m.send_status {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(status).small().color(p.muted));
                                        if status == "Sent · awaiting sync"
                                            && ui.small_button("Dismiss").clicked()
                                        {
                                            thread_clicked = true;
                                            self.app.dismiss_outgoing(&m.id);
                                        }
                                    });
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
                                    if !synced {
                                        ui.label(a["fileName"].as_str().unwrap_or("Attachment"));
                                    } else if crate::images::is_image(a) {
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
                                let reactions = message_reaction_groups(
                                    m,
                                    self.app.status["userID"].as_str().unwrap_or(""),
                                );
                                if !reactions.is_empty() {
                                    ui.horizontal_wrapped(|ui| {
                                        for summary in reactions {
                                            let response = ui.add(paint_reaction(
                                                self.reaction_button(
                                                    ui,
                                                    &summary.key,
                                                    summary.image.as_deref(),
                                                    summary.shortcode.as_deref(),
                                                    &reaction_count_label(
                                                        summary.count,
                                                        summary.mine,
                                                    ),
                                                ),
                                                summary.mine,
                                                p,
                                            ));
                                            if response.clicked() {
                                                reaction_clicked.set(true);
                                                self.reaction_target = None;
                                                self.reaction_detail = Some(ReactionDetail {
                                                    chat: chat.clone(),
                                                    message: m.clone(),
                                                    anchor: response.rect.center_bottom()
                                                        + egui::vec2(0., 6.),
                                                });
                                            }
                                        }
                                        if can_react {
                                            let add = message_action(ui, MessageAction::React, p);
                                            if add.clicked() {
                                                reaction_clicked.set(true);
                                                self.open_reaction_picker(
                                                    chat,
                                                    m,
                                                    (&add).into(),
                                                    add.layer_id,
                                                    ui.ctx(),
                                                );
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
        let hovered = ui.rect_contains_pointer(response.rect);
        let show_actions = hovered
            || egui::Popup::is_id_open(ui.ctx(), more_id)
            || self
                .reaction_target
                .as_ref()
                .is_some_and(|picker| picker.message.id == m.id);
        let shortcut = if hovered
            && matches!(self.app.mode, Mode::Browse | Mode::Compose)
            && self.gallery.is_empty()
            && self.reaction_target.is_none()
            && self.reaction_detail.is_none()
            && !egui::Popup::is_any_open(ui.ctx())
            && ui
                .memory(|memory| memory.focused())
                .is_none_or(|id| egui::TextEdit::load_state(ui.ctx(), id).is_none())
        {
            message_shortcut(ui, can_reply, can_edit)
        } else {
            None
        };
        let can_focus_composer = chat.can_send && !self.file_busy;
        let focus_composer = |ui: &egui::Ui| {
            if can_focus_composer {
                ui.memory_mut(|memory| memory.request_focus(composer_id(chat)));
            }
        };
        let fill = if m.is_sender { p.own } else { p.bg };
        ui.painter().set(
            background,
            egui::Shape::rect_filled(
                response.rect,
                12.,
                if show_actions {
                    fill.lerp_to_gamma(p.raised, 0.55)
                } else {
                    fill
                },
            ),
        );
        let right = response.rect.right().min(ui.clip_rect().right()) - 10.;
        let toolbar_rect = egui::Rect::from_min_size(
            egui::pos2(right - toolbar_width, response.rect.top() + 6.),
            egui::vec2(toolbar_width, ACTION_SIZE + 2. * (ACTION_PADDING + 1.)),
        );
        let mut toolbar = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(actions_id)
                .max_rect(toolbar_rect),
        );
        if show_actions {
            egui::Frame::new()
                .fill(p.bg)
                .stroke(egui::Stroke::new(1.0_f32, p.line))
                .corner_radius(8)
                .inner_margin(ACTION_PADDING)
                .show(&mut toolbar, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = ACTION_GAP;
                        if show_quick_reactions {
                            let self_id =
                                self.app.status["userID"].as_str().unwrap_or("").to_owned();
                            for (key, label) in
                                [("👍", "Thumbs up"), ("❤️", "Heart"), ("🔥", "Fire")]
                            {
                                let mine = user_has_reaction(&m.reactions, key, &self_id);
                                let response = icon_button(ui, ACTION_SIZE, label, mine, p);
                                let icon_rect = egui::Rect::from_center_size(
                                    response.rect.center(),
                                    Vec2::splat(18.),
                                );
                                if let Some(image) = emoji::icon(ui, key, 18.) {
                                    image.paint_at(ui, icon_rect);
                                }
                                if response.clicked() {
                                    self.react(chat, m, key, mine);
                                }
                            }
                            let (divider, _) =
                                ui.allocate_exact_size(egui::vec2(1., 15.), egui::Sense::hover());
                            ui.painter().line_segment(
                                [divider.center_top(), divider.center_bottom()],
                                egui::Stroke::new(1.0_f32, p.line),
                            );
                        }
                        if can_react {
                            let add = message_action(ui, MessageAction::React, p);
                            if add.clicked() {
                                self.open_reaction_picker(
                                    chat,
                                    m,
                                    (&add).into(),
                                    add.layer_id,
                                    ui.ctx(),
                                );
                            }
                        }
                        if can_reply
                            && (message_action(ui, MessageAction::Reply, p).clicked()
                                || shortcut == Some(egui::Key::R))
                        {
                            self.reply_to(index);
                            if self.app.reply.is_some() {
                                focus_composer(ui);
                            }
                        }
                        if can_edit
                            && (message_action(ui, MessageAction::Edit, p).clicked()
                                || shortcut == Some(egui::Key::E))
                        {
                            self.app.message_selection = index;
                            self.app.editor.set(m.text.clone());
                            self.app.edit_message = Some(m.id.clone());
                            self.app.mode = Mode::Compose;
                            focus_composer(ui);
                        }
                        let more = message_action(ui, MessageAction::More, p);
                        egui::Popup::menu(&more).id(more_id).show(|ui| {
                            if synced
                                && chat.source == "slack"
                                && chat.kind == "chat"
                                && chat.thread_root_id.is_none()
                                && ui.button("View full thread").clicked()
                            {
                                self.app.open_thread(m);
                                ui.close();
                            }
                            if ui.button("Copy message").clicked() {
                                ui.ctx().copy_text(m.text.clone());
                                ui.close();
                            }
                            if let Some(id) = &m.sender_contact_id
                                && ui.button("View profile").clicked()
                            {
                                self.app.open_contact(json!({"id":id}));
                                ui.close();
                            }
                        });
                    });
                });
        }
        if has_thread
            && !reaction_clicked.get()
            && (response.clicked() || thread_clicked)
            && !response.double_clicked()
        {
            let target = (chat.id.clone(), m.id.clone());
            if self.expanded_thread.as_ref() == Some(&target) {
                self.expanded_thread = None;
            } else {
                self.expanded_thread = Some(target);
                self.app.load_inline_thread(&m.id);
            }
        }
        if synced && chat.can_react {
            let activation_pos = ui.input(|input| {
                input.pointer.interact_pos().filter(|pos| {
                    response.rect.contains(*pos)
                        && (!show_actions || !toolbar_rect.contains(*pos))
                        && (input
                            .pointer
                            .button_double_clicked(egui::PointerButton::Primary)
                            || input.pointer.button_clicked(egui::PointerButton::Secondary))
                })
            });
            if let Some(pos) =
                activation_pos.filter(|pos| ui.ctx().layer_id_at(*pos) == Some(ui.layer_id()))
            {
                self.open_reaction_picker(chat, m, pos.into(), ui.layer_id(), ui.ctx());
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
        self.app.queue_reaction(&chat.id, &m.id, key, !remove);
    }
    fn show_reaction_detail(&mut self, ctx: &egui::Context) {
        let Some(detail) = self.reaction_detail.clone() else {
            return;
        };
        if self.app.active.as_ref().map(|chat| chat.id.as_str()) != Some(detail.chat.id.as_str()) {
            self.reaction_detail = None;
            return;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.reaction_detail = None;
            return;
        }
        let message = self
            .app
            .messages
            .iter()
            .find(|item| item.id == detail.message.id)
            .cloned()
            .unwrap_or(detail.message.clone());
        let self_id = self.app.status["userID"].as_str().unwrap_or("").to_owned();
        let p = self.theme.palette(ctx.theme());
        let mut open = true;
        let mut picked = false;
        let below = detail.anchor.y + 360. <= ctx.content_rect().bottom();
        let shown = egui::Window::new("Reactions")
            .id(egui::Id::new(("reaction_detail", message.id.clone())))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(300.)
            .pivot(if below {
                egui::Align2::LEFT_TOP
            } else {
                egui::Align2::LEFT_BOTTOM
            })
            .fixed_pos(detail.anchor)
            .show(ctx, |ui| {
                ui.set_min_width(280.);
                ui.label(RichText::new("Quick").small().color(p.muted));
                let groups = message_reaction_groups(&message, &self_id);
                let mut keys = Vec::new();
                for group in groups.iter().take(12) {
                    keys.push((
                        group.key.clone(),
                        group.image.clone(),
                        group.shortcode.clone(),
                    ));
                }
                for key in QUICK_REACTIONS {
                    if !keys.iter().any(|(known, _, _)| known == key) {
                        keys.push(((*key).to_owned(), None, None));
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    for (key, image, shortcode) in &keys {
                        let mine = message
                            .reactions
                            .iter()
                            .any(|reaction| reaction_is_users(reaction, key, &self_id));
                        let button = paint_reaction(
                            self.reaction_button(
                                ui,
                                key,
                                image.as_deref(),
                                shortcode.as_deref(),
                                if mine { "you" } else { "" },
                            ),
                            mine,
                            p,
                        );
                        if ui.add_enabled(detail.chat.can_react, button).clicked() {
                            self.react(&detail.chat, &message, key, mine);
                            picked = true;
                        }
                    }
                });
                ui.separator();
                ui.label(RichText::new("Who reacted").small().color(p.muted));
                egui::ScrollArea::vertical()
                    .max_height(340.)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_max_width(300.);
                        if groups.is_empty() {
                            ui.label(RichText::new("No reactions yet").color(p.muted));
                        }
                        for group in &groups {
                            ui.add_space(6.);
                            ui.horizontal(|ui| {
                                let _ = ui.add(paint_reaction(
                                    self.reaction_button(
                                        ui,
                                        &group.key,
                                        group.image.as_deref(),
                                        group.shortcode.as_deref(),
                                        &reaction_count_label(group.count, group.mine),
                                    ),
                                    group.mine,
                                    p,
                                ));
                                if let Some(name) = group
                                    .shortcode
                                    .as_deref()
                                    .map(emoji_name)
                                    .filter(|name| !name.is_empty())
                                {
                                    ui.label(RichText::new(name).color(p.muted).size(12.));
                                }
                            });
                            for name in &group.names {
                                let you = name == "You";
                                let mut text = RichText::new(name);
                                if you {
                                    text = text.strong().color(p.accent);
                                }
                                ui.add(egui::Label::new(text).truncate());
                            }
                            if group.unnamed > 0 {
                                ui.label(
                                    RichText::new(format!(
                                        "{} unnamed {}",
                                        group.unnamed,
                                        if group.unnamed == 1 {
                                            "person"
                                        } else {
                                            "people"
                                        }
                                    ))
                                    .small()
                                    .color(p.muted),
                                );
                            }
                        }
                    });
            });
        if shown.is_some_and(|shown| shown.response.clicked_elsewhere()) {
            open = false;
        }
        if !open || picked {
            self.reaction_detail = None;
        }
    }
    fn reaction_button(
        &mut self,
        ui: &egui::Ui,
        key: &str,
        image: Option<&str>,
        shortcode: Option<&str>,
        label: &str,
    ) -> egui::Button<'static> {
        let mxc = image
            .filter(|value| value.starts_with("mxc://"))
            .or_else(|| key.starts_with("mxc://").then_some(key));
        if let Some(mxc) = mxc {
            let cache_key = format!("emoji:{mxc}");
            self.media.request(
                Request::Emoji(mxc.to_owned()),
                self.app.api.clone(),
                ui.ctx(),
            );
            if let Some(texture) = self.media.texture(&cache_key) {
                let image = egui::Image::new((texture.id(), egui::vec2(22., 22.)));
                return egui::Button::image_and_text(image, label.to_owned());
            }
            let name = shortcode
                .map(emoji_name)
                .filter(|name| !name.is_empty())
                .unwrap_or("emoji");
            let text = if label.is_empty() {
                name.to_owned()
            } else {
                format!("{name} {label}")
            };
            return egui::Button::new(text);
        }
        emoji::button(ui, key, label)
    }
    fn open_reaction_picker(
        &mut self,
        chat: &Conversation,
        message: &Message,
        anchor: egui::PopupAnchor,
        layer_id: egui::LayerId,
        ctx: &egui::Context,
    ) {
        self.reaction_detail = None;
        self.load_discord_emoji(chat, ctx);
        if matches!(chat.source.as_str(), "slack" | "discord") {
            self.load_composer_room(chat, ctx);
        }
        self.reaction_target = Some(ReactionPicker {
            chat: chat.clone(),
            message: message.clone(),
            anchor,
            layer_id,
            search: String::new(),
            focus_search: true,
        });
    }

    fn load_discord_emoji(&mut self, chat: &Conversation, ctx: &egui::Context) {
        let (Some(guild_id), Some(api)) = (chat.server_id.as_ref(), self.app.api.clone()) else {
            return;
        };
        if chat.source != "discord"
            || chat.remote_id.is_empty()
            || self.discord_emoji.get(guild_id).is_some_and(|result| {
                result.is_none()
                    || self
                        .discord_emoji_requested
                        .get(guild_id)
                        .is_some_and(|requested| {
                            let max_age = if matches!(result, Some(Ok(_))) {
                                300
                            } else {
                                10
                            };
                            requested.elapsed() < Duration::from_secs(max_age)
                        })
            })
        {
            return;
        }
        self.discord_emoji.entry(guild_id.clone()).or_insert(None);
        self.discord_emoji_requested
            .insert(guild_id.clone(), std::time::Instant::now());
        let chat_id = guild_id.clone();
        let path = discord_emoji_path(&chat.remote_id);
        let tx = self.media.tx.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let result = async {
                let page = api.get(&path).await?;
                anyhow::Ok(serde_json::from_value(page["items"].clone())?)
            }
            .await
            .map_err(|err| err.to_string());
            let _ = tx.send(Event::DiscordEmojis(chat_id, result));
            ctx.request_repaint();
        });
    }

    fn observe_reaction_usage(&mut self) {
        if self.app.demo {
            return;
        }
        let self_id = self.app.status["userID"].as_str().unwrap_or("");
        let mut changed = false;
        for message in &self.app.messages {
            for reaction in &message.reactions {
                let Some(key) = reaction["reactionKey"].as_str() else {
                    continue;
                };
                if !message.id.is_empty()
                    && reaction["local"] != true
                    && reaction_is_users(reaction, key, self_id)
                {
                    changed |= self
                        .emoji_usage
                        .entry(key.to_owned())
                        .or_default()
                        .insert(message.id.clone());
                }
            }
        }
        if changed {
            self.save_settings();
        }
    }

    fn reaction_picker(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.reaction_target.take() else {
            return;
        };
        if self.app.active.as_ref().map(|chat| &chat.id) != Some(&picker.chat.id)
            || ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            return;
        }
        let message = self
            .app
            .messages
            .iter()
            .find(|message| message.id == picker.message.id)
            .unwrap_or(&picker.message)
            .clone();
        let self_id = self.app.status["userID"].as_str().unwrap_or("").to_owned();
        let p = self.theme.palette(ctx.theme());
        let mut open = true;
        let mut picked = None;
        let mut resolve_discord = None;
        let id = egui::Id::new("reaction-picker");
        let server_emoji = picker
            .chat
            .server_id
            .as_ref()
            .and_then(|id| self.discord_emoji.get(id))
            .cloned();
        let mut custom = BTreeMap::new();
        if let Some(items) = self.room_custom_emoji.get(mention_room_key(&picker.chat)) {
            for emoji in items {
                if let Some(mxc) = &emoji.mxc {
                    custom.insert(mxc.clone(), (emoji.name.clone(), mxc.clone()));
                }
            }
        }
        for reaction in self
            .app
            .messages
            .iter()
            .flat_map(|message| &message.reactions)
        {
            let (Some(key), Some(name)) = (
                reaction["reactionKey"].as_str(),
                reaction["shortcode"].as_str(),
            ) else {
                continue;
            };
            let image = reaction["image"].as_str().unwrap_or(key);
            if image.starts_with("mxc://") {
                custom
                    .entry(key.to_owned())
                    .or_insert_with(|| (emoji_name(name).to_owned(), image.to_owned()));
            }
        }
        let mut most_used = self
            .emoji_usage
            .iter()
            .filter_map(|(key, uses)| {
                let custom_emoji = custom.get(key);
                let server = server_emoji
                    .as_ref()
                    .and_then(Option::as_ref)
                    .and_then(|result| result.as_ref().ok())
                    .and_then(|items| items.iter().find(|emoji| emoji.mxc.as_ref() == Some(key)));
                let (name, image) = if let Some((name, mxc)) = custom_emoji {
                    (name.as_str(), Some(mxc.as_str()))
                } else if let Some(emoji) = server {
                    (emoji.name.as_str(), Some(key.as_str()))
                } else if !key.starts_with("mxc://") {
                    (emoji::reaction_name(key).unwrap_or(key), None)
                } else {
                    return None;
                };
                Some((
                    key.clone(),
                    name.to_owned(),
                    image.map(str::to_owned),
                    uses.len(),
                ))
            })
            .collect::<Vec<_>>();
        most_used.sort_by_key(|entry| std::cmp::Reverse(entry.3));
        egui::Popup::new(id, ctx.clone(), picker.anchor, picker.layer_id)
            .open_bool(&mut open)
            .align(egui::RectAlign::TOP_END)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .gap(6.)
            .width(324.)
            .frame(
                egui::Frame::popup(&ctx.style())
                    .fill(p.panel)
                    .stroke(egui::Stroke::new(1.0_f32, p.line))
                    .inner_margin(10.)
                    .corner_radius(10),
            )
            .show(|ui| {
                ui.set_width(304.);
                let search = ui.add(
                    egui::TextEdit::singleline(&mut picker.search)
                        .hint_text("Search emoji")
                        .desired_width(ui.available_width()),
                );
                if std::mem::take(&mut picker.focus_search) {
                    search.request_focus();
                }
                let query = picker.search.trim().trim_matches(':').to_lowercase();
                ui.separator();
                let mut hovered_name = None;
                let mut found = false;
                egui::ScrollArea::vertical()
                    .max_height(230.)
                    .min_scrolled_height(230.)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(3., 3.);
                        if query.is_empty() && !most_used.is_empty() {
                            found = true;
                            ui.label(RichText::new("Most used").small().strong().color(p.muted));
                            ui.horizontal_wrapped(|ui| {
                                for (key, name, image, _) in most_used.iter().take(12) {
                                    let mine = user_has_reaction(&message.reactions, key, &self_id);
                                    let cell = icon_button(ui, 32., name, mine, p);
                                    if let Some(image) = image {
                                        self.picker_custom_icon(
                                            ui,
                                            &cell,
                                            Request::Emoji(image.clone()),
                                            p,
                                        );
                                    } else if let Some(image) = emoji::icon(ui, key, 23.) {
                                        paint_picker_icon(ui, &cell, image);
                                    } else {
                                        ui.painter().text(
                                            cell.rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            key,
                                            egui::FontId::proportional(19.),
                                            p.text,
                                        );
                                    }
                                    if cell.hovered() {
                                        hovered_name = Some(name.as_str());
                                    }
                                    if cell.clicked() {
                                        picked = Some((key.clone(), mine));
                                    }
                                }
                            });
                            ui.add_space(5.);
                        }
                        if let Some(server_emoji) = &server_emoji {
                            match server_emoji {
                                Some(Ok(items)) => {
                                    let mut matches = items
                                        .iter()
                                        .filter(|emoji| {
                                            emoji::matches_search(&emoji.id, &emoji.name, &query)
                                        })
                                        .peekable();
                                    if matches.peek().is_some() {
                                        found = true;
                                        ui.label(
                                            RichText::new(
                                                picker
                                                    .chat
                                                    .server_name
                                                    .as_deref()
                                                    .unwrap_or("Discord server emoji"),
                                            )
                                            .small()
                                            .strong()
                                            .color(p.muted),
                                        );
                                        ui.horizontal_wrapped(|ui| {
                                            for emoji in matches {
                                                let mine =
                                                    emoji.mxc.as_deref().is_some_and(|key| {
                                                        user_has_reaction(
                                                            &message.reactions,
                                                            key,
                                                            &self_id,
                                                        )
                                                    });
                                                let cell =
                                                    icon_button(ui, 32., &emoji.name, mine, p);
                                                if ui.is_rect_visible(cell.rect) {
                                                    self.picker_custom_icon(
                                                        ui,
                                                        &cell,
                                                        Request::DiscordEmoji(emoji.id.clone()),
                                                        p,
                                                    );
                                                }
                                                if cell.hovered() {
                                                    hovered_name = Some(emoji.name.as_str());
                                                }
                                                if cell.clicked() {
                                                    if let Some(key) = &emoji.mxc {
                                                        picked = Some((key.clone(), mine));
                                                    } else {
                                                        resolve_discord = Some(emoji.id.clone());
                                                    }
                                                }
                                            }
                                        });
                                        ui.add_space(5.);
                                    }
                                }
                                Some(Err(error)) => {
                                    ui.label(
                                        RichText::new("Discord server emoji unavailable")
                                            .small()
                                            .color(p.muted),
                                    )
                                    .on_hover_text(error);
                                }
                                None => {
                                    ui.label(
                                        RichText::new("Loading Discord server emoji…")
                                            .small()
                                            .color(p.muted),
                                    );
                                }
                            }
                        }
                        let mut matches = custom
                            .iter()
                            .filter(|(key, (name, _))| emoji::matches_search(key, name, &query))
                            .peekable();
                        if matches.peek().is_some() {
                            found = true;
                            ui.label(
                                RichText::new("Conversation emoji")
                                    .small()
                                    .strong()
                                    .color(p.muted),
                            );
                            ui.horizontal_wrapped(|ui| {
                                for (key, (name, mxc)) in matches {
                                    let mine = user_has_reaction(&message.reactions, key, &self_id);
                                    let cell = icon_button(ui, 32., name, mine, p);
                                    if ui.is_rect_visible(cell.rect) {
                                        self.picker_custom_icon(
                                            ui,
                                            &cell,
                                            Request::Emoji(mxc.clone()),
                                            p,
                                        );
                                    }
                                    if cell.hovered() {
                                        hovered_name = Some(name);
                                    }
                                    if cell.clicked() {
                                        picked = Some((key.clone(), mine));
                                    }
                                }
                            });
                        }
                        for (title, entries) in emoji::catalog() {
                            let matches: Vec<_> = entries
                                .iter()
                                .filter(|entry| entry.matches(&query))
                                .collect();
                            if matches.is_empty() {
                                continue;
                            }
                            found = true;
                            ui.label(RichText::new(*title).small().strong().color(p.muted));
                            emoji_grid(ui, &matches, |ui, entry| {
                                let mine =
                                    user_has_reaction(&message.reactions, entry.glyph, &self_id);
                                let cell = icon_button(ui, 32., entry.name, mine, p);
                                if let Some(image) = emoji::icon(ui, entry.glyph, 23.) {
                                    paint_picker_icon(ui, &cell, image);
                                }
                                if cell.hovered() {
                                    hovered_name = Some(entry.name);
                                }
                                if cell.clicked() {
                                    picked = Some((entry.glyph.to_owned(), mine));
                                }
                            });
                            ui.add_space(5.);
                        }
                        if !found {
                            ui.label(RichText::new("No matching emoji").color(p.muted));
                        }
                    });
                ui.separator();
                ui.add(
                    egui::Label::new(
                        RichText::new(hovered_name.unwrap_or("Choose an emoji"))
                            .small()
                            .color(p.muted),
                    )
                    .truncate(),
                );
            });
        if let Some((key, mine)) = picked {
            self.react(&picker.chat, &message, &key, mine);
        } else if let Some(emoji_id) = resolve_discord {
            if let Some(api) = self.app.api.clone() {
                let path = format!(
                    "{}&emojiID={}",
                    discord_emoji_path(&picker.chat.remote_id),
                    urlencoding::encode(&emoji_id)
                );
                let chat_id = picker.chat.id;
                let message_id = message.id;
                let tx = self.media.tx.clone();
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    let result = async {
                        let emoji = api.get(&path).await?;
                        emoji["mxc"]
                            .as_str()
                            .filter(|key| key.starts_with("mxc://"))
                            .map(str::to_owned)
                            .ok_or_else(|| anyhow::anyhow!("Missing Discord emoji media"))
                    }
                    .await
                    .map_err(|err| err.to_string());
                    let _ = tx.send(Event::DiscordReaction(chat_id, message_id, result));
                    ctx.request_repaint();
                });
            }
        } else if open {
            self.reaction_target = Some(picker);
        }
    }

    fn picker_custom_icon(
        &mut self,
        ui: &egui::Ui,
        cell: &egui::Response,
        request: Request,
        p: Palette,
    ) {
        let key = request.key();
        self.media.request(request, self.app.api.clone(), ui.ctx());
        if let Some(texture) = self.media.texture(&key) {
            paint_picker_icon(ui, cell, egui::Image::new((texture.id(), Vec2::splat(23.))));
        } else {
            ui.painter().text(
                cell.rect.center(),
                egui::Align2::CENTER_CENTER,
                "…",
                egui::FontId::proportional(15.),
                p.muted,
            );
        }
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
    fn load_composer_room(&mut self, chat: &Conversation, ctx: &egui::Context) {
        let Some(api) = self.app.api.clone() else {
            return;
        };
        let key = mention_room_key(chat).to_owned();
        if self
            .mention_members
            .get(&key)
            .is_some_and(|(requested, _)| requested.elapsed() < Duration::from_secs(60))
        {
            return;
        }
        self.mention_members
            .entry(key.clone())
            .and_modify(|(requested, _)| *requested = std::time::Instant::now())
            .or_insert_with(|| (std::time::Instant::now(), Vec::new()));
        let path = Api::conversation(&chat.id, "details");
        let tx = self.media.tx.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            if let Ok(response) = api.get(&path).await
                && let Ok(members) =
                    serde_json::from_value(response["participants"]["items"].clone())
            {
                let emoji =
                    serde_json::from_value(response["customEmoji"].clone()).unwrap_or_default();
                let _ = tx.send(Event::ComposerRoom(key, members, emoji));
                ctx.request_repaint();
            }
        });
    }

    fn mention_suggestions(&self, chat: &Conversation, query: &str) -> Vec<ComposerSuggestion> {
        let mut names = BTreeMap::new();
        let self_id = self.app.status["userID"].as_str();
        if let Some((_, members)) = self.mention_members.get(mention_room_key(chat)) {
            for member in members {
                if !member.is_self
                    && Some(member.id.as_str()) != self_id
                    && valid_mention_id(&member.id)
                    && member.full_name != member.id
                {
                    names.insert(
                        member.id.as_str(),
                        (
                            member.full_name.as_str(),
                            member
                                .aliases
                                .iter()
                                .map(String::as_str)
                                .collect::<Vec<_>>(),
                        ),
                    );
                }
            }
        }
        for message in &self.app.messages {
            if !message.is_sender
                && let Some(id) = message.sender_id.as_deref()
                && Some(id) != self_id
                && valid_mention_id(id)
            {
                let entry = names
                    .entry(id)
                    .or_insert((&message.sender_name, Vec::new()));
                if entry.0 != message.sender_name
                    && !entry.1.contains(&message.sender_name.as_str())
                {
                    entry.1.push(&message.sender_name);
                }
            }
        }
        let mut name_counts = BTreeMap::new();
        for (name, _) in names.values() {
            *name_counts
                .entry(one_line(name).to_lowercase())
                .or_insert(0usize) += 1;
        }
        let mut suggestions = Vec::new();
        for (&id, (name, aliases)) in &names {
            let name = one_line(name);
            let normalized = name.to_lowercase();
            let id_lower = id.to_ascii_lowercase();
            let matching_alias = aliases
                .iter()
                .map(|alias| alias.to_lowercase())
                .find(|alias| alias.contains(query));
            if name.trim().is_empty()
                || (!normalized.contains(query)
                    && !id_lower.contains(query)
                    && matching_alias.is_none())
            {
                continue;
            }
            let display = if name_counts[&normalized] > 1 {
                let handle = id.trim_start_matches('@').split(':').next().unwrap_or(id);
                format!("{name} ({handle})")
            } else {
                name
            };
            suggestions.push(ComposerSuggestion {
                name: display.clone(),
                insert: format!("@{display}"),
                image: None,
                user_id: Some(id.to_owned()),
                uses: 0,
                keywords: if normalized.starts_with(query) {
                    normalized
                } else {
                    matching_alias
                        .filter(|alias| alias.starts_with(query))
                        .unwrap_or(normalized)
                },
            });
        }
        suggestions.sort_by(|a, b| {
            (!a.keywords.starts_with(query), &a.keywords, &a.name).cmp(&(
                !b.keywords.starts_with(query),
                &b.keywords,
                &b.name,
            ))
        });
        suggestions
    }

    fn complete_composer(
        &mut self,
        ui: &egui::Ui,
        chat: &Conversation,
        edit: &mut egui::text_edit::TextEditOutput,
        mut menu: ComposerCompletionMenu,
        action: Option<egui::Key>,
        p: Palette,
    ) {
        let popup_id = edit.response.id.with("composer-completion");
        let store = |menu: ComposerCompletionMenu| {
            ui.ctx().data_mut(|data| data.insert_temp(popup_id, menu));
        };
        let over_popup = ui.ctx().pointer_hover_pos().is_some_and(|pos| {
            ui.ctx().layer_id_at(pos) == Some(egui::LayerId::new(egui::Order::Foreground, popup_id))
        });
        let Some(cursor) = edit.cursor_range.filter(|range| range.is_empty()) else {
            store(ComposerCompletionMenu::default());
            return;
        };
        let text = &self.app.editor.text;
        let mention = (chat.kind == "chat")
            .then(|| emoji::completion_token_for(text, cursor.primary.index, '@'))
            .flatten();
        let is_mention = mention.is_some();
        let Some((range, query)) =
            mention.or_else(|| emoji::completion_token(text, cursor.primary.index))
        else {
            store(ComposerCompletionMenu::default());
            return;
        };
        let query = query.to_lowercase();
        if !(edit.response.has_focus() || menu.visible && over_popup) {
            store(ComposerCompletionMenu::default());
            return;
        }
        let query_changed = menu.start != range.start || menu.query != query;
        if query_changed {
            menu = ComposerCompletionMenu {
                start: range.start,
                query: query.clone(),
                ..Default::default()
            };
        }
        if action == Some(egui::Key::Escape) {
            menu.dismissed = true;
        }
        if menu.dismissed {
            menu.visible = false;
            store(menu);
            return;
        }

        let suggestions = if is_mention {
            self.load_composer_room(chat, ui.ctx());
            self.mention_suggestions(chat, &query)
        } else {
            self.load_discord_emoji(chat, ui.ctx());
            if matches!(chat.source.as_str(), "slack" | "discord") {
                self.load_composer_room(chat, ui.ctx());
            }
            let discord = chat
                .server_id
                .as_ref()
                .and_then(|id| self.discord_emoji.get(id))
                .and_then(Option::as_ref)
                .and_then(|result| result.as_ref().ok());
            let mut suggestions = Vec::new();
            if let Some(discord) = discord {
                for emoji in discord {
                    suggestions.push(ComposerSuggestion {
                        name: emoji.name.clone(),
                        insert: format!(":{}:", emoji.name),
                        image: Some(Request::DiscordEmoji(emoji.id.clone())),
                        user_id: None,
                        uses: emoji
                            .mxc
                            .as_ref()
                            .and_then(|key| self.emoji_usage.get(key))
                            .map_or(0, BTreeSet::len),
                        keywords: emoji.name.clone(),
                    });
                }
            }
            if matches!(chat.source.as_str(), "discord" | "slack") {
                let mut known = BTreeMap::new();
                if let Some(items) = self.room_custom_emoji.get(mention_room_key(chat)) {
                    for item in items {
                        known.insert(item.name.as_str(), (item.mxc.as_deref(), item.thumbnail));
                    }
                }
                for message in &self.app.messages {
                    for emote in &message.emotes {
                        known
                            .entry(&emote.name)
                            .or_insert((Some(&emote.mxc), false));
                    }
                    for reaction in &message.reactions {
                        let name = reaction["shortcode"].as_str().map(emoji_name);
                        let image = reaction["image"]
                            .as_str()
                            .or_else(|| reaction["reactionKey"].as_str());
                        if let (Some(name), Some(image)) = (name, image)
                            && image.starts_with("mxc://")
                        {
                            known.entry(name).or_insert((Some(image), false));
                        }
                    }
                }
                let catalog_names = suggestions
                    .iter()
                    .map(|item| item.name.to_lowercase())
                    .collect::<BTreeSet<_>>();
                suggestions.extend(known.into_iter().filter_map(|(name, (mxc, thumbnail))| {
                    if name.is_empty() || catalog_names.contains(&name.to_lowercase()) {
                        return None;
                    }
                    let image = mxc.map(|mxc| Request::Emoji(mxc.to_owned())).or_else(|| {
                        (thumbnail && !chat.remote_id.is_empty()).then(|| Request::SlackEmoji {
                            room: chat.remote_id.clone(),
                            name: name.to_owned(),
                        })
                    });
                    Some(ComposerSuggestion {
                        name: name.to_owned(),
                        insert: format!(":{name}:"),
                        image,
                        user_id: None,
                        uses: mxc
                            .and_then(|key| self.emoji_usage.get(key))
                            .map_or(0, BTreeSet::len),
                        keywords: name.to_owned(),
                    })
                }));
            }
            suggestions.retain(|item| {
                item.name.starts_with(&query)
                    || emoji::matches_search(&item.insert, &item.keywords, &query)
                    || item.name.contains(&query)
            });
            for (_, entries) in emoji::catalog() {
                suggestions.extend(entries.iter().filter(|entry| entry.matches(&query)).map(
                    |entry| ComposerSuggestion {
                        name: entry.shortcode.clone(),
                        insert: entry.glyph.to_owned(),
                        image: None,
                        user_id: None,
                        uses: self.emoji_usage.get(entry.glyph).map_or(0, BTreeSet::len),
                        keywords: String::new(),
                    },
                ));
            }
            suggestions.sort_by_key(|item| {
                (
                    !item.name.starts_with(&query),
                    item.name != query,
                    query.is_empty() && !item.insert.starts_with(':'),
                    std::cmp::Reverse(item.uses),
                )
            });
            suggestions
        };
        if suggestions.is_empty() {
            menu.visible = false;
            store(menu);
            return;
        }
        let count = suggestions.len();
        menu.selected = menu.selected.min(count - 1);
        match action {
            Some(egui::Key::ArrowUp) => menu.selected = (menu.selected + count - 1) % count,
            Some(egui::Key::ArrowDown) => menu.selected = (menu.selected + 1) % count,
            _ => {}
        }
        let mut picked =
            matches!(action, Some(egui::Key::Tab | egui::Key::Enter)).then_some(menu.selected);
        let width = edit.response.rect.width().min(350.);
        egui::Popup::new(
            popup_id,
            ui.ctx().clone(),
            egui::PopupAnchor::from(&edit.response),
            ui.layer_id(),
        )
        .align(egui::RectAlign::TOP_START)
        .gap(5.)
        .width(width)
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .frame(
            egui::Frame::popup(ui.style())
                .fill(p.panel)
                .stroke(egui::Stroke::new(1.0_f32, p.line))
                .inner_margin(6)
                .corner_radius(8),
        )
        .show(|ui| {
            ui.set_width((width - 14.).max(80.));
            ui.spacing_mut().item_spacing.y = 1.;
            let mut scroll = egui::ScrollArea::vertical().max_height(223.);
            if query_changed || !menu.visible {
                scroll = scroll.vertical_scroll_offset(0.);
            } else if matches!(action, Some(egui::Key::ArrowUp | egui::Key::ArrowDown)) {
                scroll =
                    scroll.vertical_scroll_offset(menu.selected.saturating_sub(4) as f32 * 28.);
            }
            scroll.show_rows(ui, 27., count, |ui, rows| {
                for index in rows {
                    let item = &suggestions[index];
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 27.),
                        egui::Sense::click(),
                    );
                    if index == menu.selected || response.hovered() {
                        ui.painter().rect_filled(rect, 5., p.raised);
                    }
                    let icon = egui::Rect::from_center_size(
                        egui::pos2(rect.left() + 17., rect.center().y),
                        Vec2::splat(19.),
                    );
                    if let Some(request) = &item.image {
                        let key = request.key();
                        self.media
                            .request(request.clone(), self.app.api.clone(), ui.ctx());
                        if let Some(texture) = self.media.texture(&key) {
                            egui::Image::new((texture.id(), Vec2::splat(19.))).paint_at(ui, icon);
                        }
                    } else if let Some(image) = emoji::icon(ui, &item.insert, 19.) {
                        image.paint_at(ui, icon);
                    }
                    let label = if item.user_id.is_some() {
                        ui.painter().text(
                            icon.center(),
                            egui::Align2::CENTER_CENTER,
                            "@",
                            egui::FontId::proportional(14.),
                            p.blue,
                        );
                        format!("@{}", item.name)
                    } else {
                        format!(":{}:", item.name)
                    };
                    ui.painter().text(
                        egui::pos2(rect.left() + 33., rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &label,
                        egui::FontId::proportional(13.),
                        p.text,
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            true,
                            index == menu.selected,
                            &label,
                        )
                    });
                    if response.clicked() {
                        picked = Some(index);
                    }
                }
            });
        });
        if let Some(index) = picked {
            let mut insert = suggestions[index].insert.clone();
            if range.end == self.app.editor.text.len() {
                insert.push(' ');
            }
            let cursor =
                self.app.editor.text[..range.start].chars().count() + insert.chars().count();
            self.app
                .editor
                .replace_completion(range, &insert, suggestions[index].user_id.clone());
            edit.state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(cursor),
                )));
            edit.state.clone().store(ui.ctx(), edit.response.id);
            edit.response.request_focus();
            self.app.mode = Mode::Compose;
            store(ComposerCompletionMenu::default());
        } else {
            menu.visible = true;
            self.composer_completion_open = true;
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    edit.response.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                );
            });
            store(menu);
        }
    }

    fn composer(&mut self, ui: &mut egui::Ui, chat: &Conversation, p: Palette) {
        if let Some(reply) = self.app.reply.clone() {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if ui
                        .small_button("×")
                        .on_hover_text("Cancel reply (Esc)")
                        .clicked()
                    {
                        self.app.reply = None;
                    }
                    let mut preview = egui::text::LayoutJob::default();
                    let font_id = egui::TextStyle::Small.resolve(ui.style());
                    preview.append(
                        &format!("Reply to {}  ", one_line(&reply.sender_name)),
                        0.,
                        egui::TextFormat::simple(font_id.clone(), p.blue),
                    );
                    preview.append(
                        &one_line(&reply.text),
                        0.,
                        egui::TextFormat::simple(font_id, p.muted),
                    );
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(preview).truncate());
                    });
                },
            );
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
        let enabled = chat.can_send && !self.file_busy;
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
                let id = composer_id(chat);
                let menu = ui
                    .ctx()
                    .data(|data| {
                        data.get_temp::<ComposerCompletionMenu>(id.with("composer-completion"))
                    })
                    .unwrap_or_default();
                let action = if enabled && menu.visible && ui.memory(|memory| memory.has_focus(id))
                {
                    ui.input_mut(|input| {
                        [
                            egui::Key::Escape,
                            egui::Key::Tab,
                            egui::Key::Enter,
                            egui::Key::ArrowDown,
                            egui::Key::ArrowUp,
                        ]
                        .into_iter()
                        .find(|key| {
                            input.modifiers.is_none()
                                && input.consume_key(egui::Modifiers::NONE, *key)
                        })
                    })
                } else {
                    None
                };
                if action == Some(egui::Key::Escape) {
                    self.app.reply = None;
                }
                let mut edit = ui
                    .add_enabled_ui(enabled, |ui| {
                        egui::TextEdit::multiline(&mut self.app.editor.text)
                            .id(composer_id(chat))
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
                self.app.editor.reconcile_mentions(&before);
                if enabled && matches!(chat.kind.as_str(), "chat" | "phone" | "email") {
                    self.complete_composer(ui, chat, &mut edit, menu, action, p);
                }
                let response = edit.response;
                self.app.editor.cursor = self.app.editor.text.len();
                if response.changed() && self.app.mode == Mode::Browse {
                    self.app.mode = Mode::Compose;
                }
                let send_key = response.has_focus()
                    && !self.composer_completion_open
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
                        let clicked = ui
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
                            .clicked();
                        if (clicked || (can_send && send_key))
                            && let Some(id) = self.app.prepare_send()
                        {
                            self.scroll_to = Some(id);
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
                    #[cfg(target_os = "macos")]
                    if self.app.login["type"] == "cookies" {
                        if ui
                            .add_enabled(
                                !self.app.login_busy,
                                egui::Button::new("Sign in with browser"),
                            )
                            .clicked()
                        {
                            self.app.web_login();
                        }
                        ui.label(
                            RichText::new("Or paste the values yourself:")
                                .small()
                                .color(p.muted),
                        );
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
    fn gallery_window(&mut self, ctx: &egui::Context, _p: Palette) {
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
        let screen = ctx.content_rect();
        let mut open = true;
        egui::Modal::new(egui::Id::new("image-lightbox"))
            .frame(egui::Frame::NONE)
            .backdrop_color(Color32::from_black_alpha(242))
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                let background =
                    ui.interact(screen, ui.id().with("dismiss-image"), egui::Sense::click());
                let content = egui::Rect::from_min_max(
                    screen.min + egui::vec2(64., 64.),
                    screen.max - egui::vec2(64., 32.),
                );
                self.media
                    .request(Request::Image(target.clone()), self.app.api.clone(), ctx);
                let ready = matches!(self.media.cache.get(&key), Some(Entry::Ready { .. }));
                let mut image_rect = egui::Rect::NOTHING;
                if let Some(texture) = self.media.texture(&key) {
                    let size = texture.size_vec2();
                    let scale = (content.width() / size.x)
                        .min(content.height() / size.y)
                        .min(1.);
                    image_rect = egui::Rect::from_center_size(content.center(), size * scale);
                    egui::Image::new(&texture).paint_at(ui, image_rect);
                } else {
                    let mut loading = ui.new_child(egui::UiBuilder::new().max_rect(content));
                    loading.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            if let Some(Entry::Failed(error)) = self.media.cache.get(&key) {
                                ui.label(RichText::new(error).color(Color32::LIGHT_GRAY));
                                if ui.button("Retry").clicked() {
                                    self.media.cache.remove(&key);
                                }
                            } else {
                                ui.spinner();
                                ui.label("Loading image…");
                            }
                        });
                    });
                }
                if background.clicked()
                    && background
                        .interact_pointer_pos()
                        .is_some_and(|pos| pos.y > screen.top() + 60. && !image_rect.contains(pos))
                {
                    open = false;
                }
                let toolbar = egui::Rect::from_min_max(
                    screen.min + egui::vec2(20., 12.),
                    egui::pos2(screen.right() - 16., screen.top() + 52.),
                );
                let mut bar = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(toolbar)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                let title_rect = egui::Rect::from_min_size(
                    toolbar.min,
                    egui::vec2((toolbar.width() - 145.).max(40.), 36.),
                );
                row_text(
                    &bar,
                    target.name(),
                    title_rect.min + egui::vec2(0., 8.),
                    title_rect.width(),
                    16.,
                    Color32::from_gray(235),
                );
                bar.allocate_rect(title_rect, egui::Sense::hover());
                bar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::gallery_button(ui, icons::GalleryIcon::Close, "Close (Esc)").clicked()
                    {
                        open = false;
                    }
                    ui.add_enabled_ui(ready, |ui| {
                        if icons::gallery_button(
                            ui,
                            icons::GalleryIcon::External,
                            "Open in browser",
                        )
                        .clicked()
                            && let Err(error) = self.media.save(&target, true)
                        {
                            self.app.notice = error;
                        }
                        if icons::gallery_button(ui, icons::GalleryIcon::Download, "Save original")
                            .clicked()
                        {
                            self.media.save_original(&target, ctx);
                        }
                    });
                });
                if self.gallery.len() > 1 {
                    for (next, x) in [(false, screen.left() + 14.), (true, screen.right() - 50.)] {
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(x, content.center().y - 18.),
                            Vec2::splat(36.),
                        );
                        let mut nav = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                        nav.add_enabled_ui(
                            if next {
                                self.gallery_index + 1 < self.gallery.len()
                            } else {
                                self.gallery_index > 0
                            },
                            |ui| {
                                if icons::gallery_button(
                                    ui,
                                    if next {
                                        icons::GalleryIcon::Next
                                    } else {
                                        icons::GalleryIcon::Previous
                                    },
                                    if next { "Next (→)" } else { "Previous (←)" },
                                )
                                .clicked()
                                {
                                    if next {
                                        self.gallery_index += 1;
                                    } else {
                                        self.gallery_index = self.gallery_index.saturating_sub(1);
                                    }
                                }
                            },
                        );
                    }
                    ui.painter().text(
                        egui::pos2(screen.center().x, screen.bottom() - 14.),
                        egui::Align2::CENTER_CENTER,
                        format!("{} / {}", self.gallery_index + 1, self.gallery.len()),
                        egui::FontId::proportional(11.),
                        Color32::from_gray(150),
                    );
                }
            });
        if !open {
            self.gallery.clear();
        }
    }
    fn handle_escape(&mut self, ctx: &egui::Context) {
        if !std::mem::take(&mut self.composer_completion_open)
            && self.gallery.is_empty()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.app.mode = if self.app.mode == Mode::Confirm {
                self.app.confirm_body = None;
                Mode::Compose
            } else {
                if matches!(self.app.mode, Mode::Browse | Mode::Compose) {
                    self.app.reply = None;
                }
                if self.app.editor.text.is_empty() {
                    Mode::Browse
                } else {
                    Mode::Compose
                }
            };
        }
    }
}
impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.reaction_picker(ctx);
        self.show_reaction_detail(ctx);
        self.app.poll();
        self.observe_reaction_usage();
        self.handle_escape(ctx);
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
                Event::ComposerRoom(room, members, emoji) => {
                    self.room_custom_emoji.insert(room.clone(), emoji);
                    if let Some((_, current)) = self.mention_members.get_mut(&room) {
                        *current = members;
                    }
                }
                Event::DiscordEmojis(guild_id, result) => {
                    self.discord_emoji.insert(guild_id, Some(result));
                }
                Event::DiscordReaction(chat_id, message_id, result) => match result {
                    Ok(key) => self.app.queue_reaction(&chat_id, &message_id, &key, true),
                    Err(error) => {
                        self.app.notice = format!("Could not load Discord emoji: {error}")
                    }
                },
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
        let p = self.theme.palette(ctx.theme());
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(16, 7)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&self.app.notice).size(11.).color(
                            if keeps_failure(&self.app.notice) {
                                p.accent
                            } else {
                                p.muted
                            },
                        ))
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
                self.app.notice = format!("Could not open the link: {error}");
            }
        }
        if let Some(mut smoke) = self.appearance_smoke.take() {
            let expected = Theme {
                mode: Some(DisplayMode::Light),
                scheme: ColorScheme::Nord,
                font: FontStyle::Terminal,
            };
            let palette_matches =
                ctx.style().visuals.text_color() == self.theme.palette(ctx.theme()).text;
            if smoke.rendered {
                smoke
                    .complete
                    .set(self.theme == expected && palette_matches);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                if matches!(smoke.mode, AppearanceSmokeTest::Write) {
                    if self.theme != Theme::default() || !palette_matches {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        return;
                    }
                    let previous = self.theme;
                    self.theme = expected;
                    self.save_theme(ctx, previous);
                }
                smoke.rendered = true;
                self.appearance_smoke = Some(smoke);
                ctx.request_repaint();
            }
        }
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}
#[derive(Clone)]
struct ReactionDetail {
    chat: Conversation,
    message: Message,
    anchor: egui::Pos2,
}
struct ReactionGroup {
    key: String,
    image: Option<String>,
    shortcode: Option<String>,
    count: usize,
    mine: bool,
    names: Vec<String>,
    unnamed: usize,
}
#[derive(Clone, Default)]
struct ComposerCompletionMenu {
    start: usize,
    query: String,
    selected: usize,
    visible: bool,
    dismissed: bool,
}

struct ComposerSuggestion {
    name: String,
    insert: String,
    image: Option<Request>,
    user_id: Option<String>,
    uses: usize,
    keywords: String,
}

struct ReactionPicker {
    chat: Conversation,
    message: Message,
    anchor: egui::PopupAnchor,
    layer_id: egui::LayerId,
    search: String,
    focus_search: bool,
}

fn discord_emoji_path(room_id: &str) -> String {
    format!(
        "/api/discord/room-emoji?roomID={}",
        urlencoding::encode(room_id)
    )
}

fn emoji_grid(
    ui: &mut egui::Ui,
    entries: &[&emoji::CatalogEmoji],
    mut draw: impl FnMut(&mut egui::Ui, &emoji::CatalogEmoji),
) {
    let size = 32.;
    let spacing = ui.spacing().item_spacing;
    let columns =
        (((ui.available_width() + spacing.x) / (size + spacing.x)).floor() as usize).max(1);
    let row_count = entries.len().div_ceil(columns);
    let pitch = size + spacing.y;
    let top = ui.next_widget_position().y;
    let clip = ui.clip_rect();
    let first = (((clip.top() - top) / pitch).floor() as usize).min(row_count);
    let end = (((clip.bottom() - top) / pitch).ceil() as usize).clamp(first, row_count);
    let skip = |ui: &mut egui::Ui, rows| {
        if rows > 0 {
            ui.allocate_space(egui::vec2(1., rows as f32 * pitch - spacing.y));
        }
    };
    skip(ui, first);
    for row in first..end {
        ui.push_id(row, |ui| {
            ui.horizontal(|ui| {
                for entry in &entries[row * columns..((row + 1) * columns).min(entries.len())] {
                    draw(ui, entry);
                }
            });
        });
    }
    skip(ui, row_count - end);
}

fn paint_picker_icon(ui: &egui::Ui, response: &egui::Response, image: egui::Image<'_>) {
    image.paint_at(
        ui,
        egui::Rect::from_center_size(response.rect.center(), Vec2::splat(23.)),
    );
}

const QUICK_REACTIONS: &[&str] = &["👍", "❤️", "😂", "🎉"];

fn emoji_name(shortcode: &str) -> &str {
    shortcode.trim().trim_matches(':')
}

const ACTION_SIZE: f32 = 26.;
const ACTION_PADDING: f32 = 3.;
const ACTION_GAP: f32 = 2.;

enum MessageAction {
    React,
    Reply,
    Edit,
    More,
}

fn mention_room_key(chat: &Conversation) -> &str {
    chat.parent_conversation_id.as_deref().unwrap_or(&chat.id)
}

fn valid_mention_id(id: &str) -> bool {
    id.starts_with('@')
        && id.contains(':')
        && !matches!(
            id.split(':').next(),
            Some("@discordbot" | "@slackbot" | "@whatsappbot")
        )
}

fn composer_id(chat: &Conversation) -> egui::Id {
    egui::Id::new(("message-composer", &chat.id))
}

fn message_shortcut(ui: &egui::Ui, can_reply: bool, can_edit: bool) -> Option<egui::Key> {
    ui.input_mut(|input| {
        let (key, letter) = input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } if modifiers.is_none() => match key {
                egui::Key::R if can_reply => Some((*key, "r")),
                egui::Key::E if can_edit => Some((*key, "e")),
                _ => None,
            },
            _ => None,
        })?;
        input.consume_key(egui::Modifiers::NONE, key);
        input
            .events
            .retain(|event| !matches!(event, egui::Event::Text(text) if text == letter));
        Some(key)
    })
}

fn icon_button(
    ui: &mut egui::Ui,
    size: f32,
    label: &str,
    selected: bool,
    p: Palette,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    if selected || response.hovered() || response.has_focus() {
        let fill = if selected {
            theme::tint(p.raised, p.accent, 0.30)
        } else {
            p.raised
        };
        ui.painter().rect_filled(rect, 5., fill);
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    response
        .on_hover_text(label)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn message_action(ui: &mut egui::Ui, action: MessageAction, p: Palette) -> egui::Response {
    let label = match action {
        MessageAction::React => "Add reaction",
        MessageAction::Reply => "Reply (R)",
        MessageAction::Edit => "Edit message (E)",
        MessageAction::More => "More actions",
    };
    let response = icon_button(ui, ACTION_SIZE, label, false, p);
    let center = response.rect.center();
    let at = |x, y| center + egui::vec2(x, y);
    let ink = if response.hovered() { p.text } else { p.muted };
    let stroke = egui::Stroke::new(1.5_f32, ink);
    let painter = ui.painter();
    match action {
        MessageAction::React => {
            painter.circle_stroke(center, 6.5, stroke);
            painter.circle_filled(at(-2.3, -1.7), 0.9, ink);
            painter.circle_filled(at(2.3, -1.7), 0.9, ink);
            painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
                [at(-2.8, 1.6), at(0., 4.8), at(2.8, 1.6)],
                false,
                Color32::TRANSPARENT,
                stroke,
            ));
        }
        MessageAction::Reply => {
            painter.add(egui::Shape::line(
                vec![at(-1.5, -6.), at(-6., -1.5), at(-1.5, 3.)],
                stroke,
            ));
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [at(-6., -1.5), at(-1., -1.5), at(6., -3.), at(6., 5.5)],
                false,
                Color32::TRANSPARENT,
                stroke,
            ));
        }
        MessageAction::Edit => {
            painter.add(egui::Shape::closed_line(
                vec![
                    at(-6., 6.),
                    at(-5., 1.5),
                    at(2.5, -6.),
                    at(6., -2.5),
                    at(-1.5, 5.),
                ],
                stroke,
            ));
            painter.line_segment([at(0.5, -4.), at(4., -0.5)], stroke);
        }
        MessageAction::More => {
            for x in [-5., 0., 5.] {
                painter.circle_filled(at(x, 0.), 1.35, ink);
            }
        }
    }
    response
}

fn reaction_count_label(count: usize, mine: bool) -> String {
    if mine {
        format!("{count} · you")
    } else {
        count.to_string()
    }
}

fn settings_choices<T: Copy + Eq>(
    ui: &mut egui::Ui,
    p: Palette,
    selected: T,
    choices: [(T, &str); 2],
) -> Option<T> {
    egui::Frame::new()
        .fill(p.bg)
        .stroke(egui::Stroke::new(1.0_f32, p.line))
        .corner_radius(8.0)
        .inner_margin(3)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let width = (ui.available_width() - 4.0) / 2.0;
                let mut choice = None;
                for (value, label) in choices {
                    let active = selected == value;
                    let button = egui::Button::selectable(active, label).corner_radius(5.0);
                    let button = if active {
                        button
                            .fill(p.select)
                            .stroke(egui::Stroke::new(1.0_f32, p.accent))
                    } else {
                        button.frame_when_inactive(false)
                    };
                    if ui.add_sized([width, 26.0], button).clicked() {
                        choice = Some(value);
                    }
                }
                choice
            })
            .inner
        })
        .inner
}

fn paint_reaction(button: egui::Button<'static>, mine: bool, p: Palette) -> egui::Button<'static> {
    button
        .fill(if mine {
            theme::tint(p.raised, p.accent, 0.62)
        } else {
            p.raised
        })
        .stroke(egui::Stroke::new(
            if mine { 2.0_f32 } else { 0.0 },
            p.accent,
        ))
        .corner_radius(10)
}

fn message_reaction_groups(message: &Message, self_id: &str) -> Vec<ReactionGroup> {
    let mut groups = reaction_groups(&message.reactions, self_id);
    let Some(counts) = &message.reaction_counts else {
        return groups;
    };
    counts
        .iter()
        .filter(|summary| summary.count > 0)
        .map(|summary| {
            let known = groups.iter().position(|group| group.key == summary.key);
            let mut group = known
                .map(|index| groups.swap_remove(index))
                .unwrap_or_else(|| ReactionGroup {
                    key: summary.key.clone(),
                    image: summary
                        .key
                        .starts_with("mxc://")
                        .then(|| summary.key.clone()),
                    shortcode: summary.shortcode.clone(),
                    count: 0,
                    mine: false,
                    names: Vec::new(),
                    unnamed: 0,
                });
            group.count = summary.count;
            group.mine = summary.mine;
            if group.shortcode.is_none() {
                group.shortcode = summary.shortcode.clone();
            }
            group.names.retain(|name| name != "You");
            if group.names.len() > summary.count - usize::from(summary.mine) {
                group.names.clear();
            }
            if summary.mine {
                group.names.insert(0, "You".into());
            }
            group.unnamed = group.count - group.names.len();
            group
        })
        .collect()
}

fn reaction_groups(reactions: &[Value], self_id: &str) -> Vec<ReactionGroup> {
    let mut groups = Vec::<ReactionGroup>::new();
    let mut seen = Vec::<(String, String)>::new();
    for reaction in reactions {
        let key = reaction["reactionKey"].as_str().unwrap_or("?").to_owned();
        let id = reaction["participantID"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned);
        if let Some(id) = id.as_ref()
            && seen
                .iter()
                .any(|(known_key, known_id)| known_key == &key && known_id == id)
        {
            continue;
        }
        if let Some(id) = id.clone() {
            seen.push((key.clone(), id));
        }
        let named = reaction["participantName"]
            .as_str()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned);
        let mine = reaction_is_users(reaction, &key, self_id);
        let image = reaction["image"]
            .as_str()
            .filter(|value| value.starts_with("mxc://"))
            .map(str::to_owned)
            .or_else(|| key.starts_with("mxc://").then(|| key.clone()));
        let shortcode = reaction["shortcode"]
            .as_str()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned);
        if !groups.iter().any(|group| group.key == key) {
            groups.push(ReactionGroup {
                key: key.clone(),
                image: image.clone(),
                shortcode: shortcode.clone(),
                count: 0,
                mine: false,
                names: Vec::new(),
                unnamed: 0,
            });
        }
        let group = groups
            .iter_mut()
            .find(|group| group.key == key)
            .expect("reaction group exists");
        if group.image.is_none() {
            group.image = image;
        }
        if group.shortcode.is_none() {
            group.shortcode = shortcode;
        }
        // Discord and Slack echo your reaction from a bridge puppet as well as
        // your Matrix user. Both are you, so the chip can only move by one.
        if mine {
            if !group.mine {
                group.count += 1;
                group.mine = true;
                if !group.names.iter().any(|name| name == "You") {
                    group.names.insert(0, "You".into());
                }
            }
        } else {
            group.count += 1;
            if let Some(name) = named {
                group.names.push(name);
            } else {
                group.unnamed += 1;
            }
        }
    }
    groups
}

fn row_text(ui: &egui::Ui, text: &str, pos: egui::Pos2, width: f32, size: f32, color: Color32) {
    let galley = row_text_galley(ui, text, width, size, color);
    ui.painter().galley(pos, galley.clone(), color);
    emoji::paint(ui, &galley, pos);
}

fn row_text_galley(
    ui: &egui::Ui,
    text: &str,
    width: f32,
    size: f32,
    color: Color32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    emoji::append(
        &mut job,
        text,
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color,
            ..Default::default()
        },
    );
    job.wrap.max_width = width.max(1.);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

// Keep loaded Slack replies beside their parent while retaining their original
// indices for reply, reaction, and edit actions. Orphan replies remain visible.
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

/// Up to two initials from the first letters of the first two words. Empty when the name
/// has no letters (a phone number), so the avatar shows a silhouette instead of a digit.
fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphabetic()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}
// Where UI preferences (the chosen theme) persist between launches. A release build's
// CARGO_MANIFEST_DIR is the build machine's checkout, which does not exist on the user's
// Mac, so a packaged app must store settings under its per-user Application Support folder.
// Other platforms and source checkouts keep using private/ui.json next to the source.
fn settings_path() -> PathBuf {
    if cfg!(all(target_os = "macos", not(test)))
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join("Library/Application Support/Chord/ui.json");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("private/ui.json")
}
pub fn run(app: App, smoke: Option<AppearanceSmokeTest>) -> anyhow::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Chord")
        .with_app_id("dev.chord.Chord")
        .with_inner_size([1240., 820.])
        .with_min_inner_size([760., 520.]);
    // Without an explicit icon, eframe uses the egui logo for the window and the Dock.
    let icon = image::load_from_memory(include_bytes!("../assets/macos/icon.png"))?.into_rgba8();
    viewport = viewport.with_icon(egui::IconData {
        width: icon.width(),
        height: icon.height(),
        rgba: icon.into_raw(),
    });
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        // Wayland already schedules presentation. Waiting for a swap interval can
        // block the event loop when a scratchpad surface is hidden.
        vsync: std::env::var_os("WAYLAND_DISPLAY").is_none(),
        ..Default::default()
    };
    let complete = std::rc::Rc::new(std::cell::Cell::new(false));
    let result = complete.clone();
    eframe::run_native(
        "Chord",
        options,
        Box::new(move |cc| {
            let mut gui = Gui::new(app, &cc.egui_ctx);
            gui.appearance_smoke = smoke.map(|mode| AppearanceSmoke {
                mode,
                rendered: false,
                complete,
            });
            Ok(Box::new(gui))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Could not open Chord: {e}"))?;
    anyhow::ensure!(
        smoke.is_none() || result.get(),
        "Default macOS appearance or saved theme settings did not render"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_style_makes_proportional_text_monospace() {
        let ctx = egui::Context::default();
        let font = egui::FontId::proportional(16.0);
        for (style, expect_equal) in [(FontStyle::Terminal, true), (FontStyle::MacOS, false)] {
            Gui::apply_fonts(&ctx, style);
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                ctx.fonts_mut(|fonts| {
                    assert_eq!(
                        fonts.glyph_width(&font, 'i') == fonts.glyph_width(&font, 'm'),
                        expect_equal
                    );
                });
            });
        }
    }

    #[test]
    fn account_filter_toggles_visibility_and_persists_it() {
        let ctx = egui::Context::default();
        let path = std::env::temp_dir().join(format!(
            "chord-account-filter-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let mut gui = Gui::with_settings_path(App::new(None, true), &ctx, path.clone());
        let frame = |gui: &mut Gui, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100., 650.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| gui.account_filter(ui, false));
                },
            )
        };
        let label = |output: &egui::FullOutput, name: &str| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == name => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
        };
        let click = |pos| {
            let mut events = vec![egui::Event::PointerMoved(pos)];
            for pressed in [true, false] {
                events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
            }
            events
        };
        let initial = frame(&mut gui, vec![]);
        frame(&mut gui, click(label(&initial, "Accounts").unwrap()));
        let menu = frame(&mut gui, vec![]);
        assert!(label(&menu, "Discord server importance").is_some());
        frame(&mut gui, click(label(&menu, "Slack").unwrap()));
        assert!(gui.app.hidden_accounts.contains("slack"));
        assert!(gui.app.visible().iter().all(|chat| chat.source != "slack"));
        let reloaded = Gui::with_settings_path(App::new(None, true), &ctx, path.clone());
        assert!(reloaded.app.hidden_accounts.contains("slack"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn narrow_inbox_keeps_accounts_filters_and_refresh_on_one_row() {
        for (width, account_label) in [(236., "≡"), (336., "Accounts")] {
            let ctx = egui::Context::default();
            let mut gui = Gui::new(App::new(None, true), &ctx);
            Gui::apply_fonts(&ctx, FontStyle::Terminal);
            let mut right_edge = 0.0;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width + 16., 400.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        // The smallest Inbox pane is 260 points including 12-point side margins.
                        ui.set_width(width);
                        right_edge = ui.max_rect().right();
                        gui.inbox_header(ui);
                    });
                },
            );
            let rect = |label| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        if let egui::Shape::Text(text) = &shape.shape
                            && text.galley.job.text == label
                        {
                            Some(text.galley.rect.translate(text.pos.to_vec2()))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| panic!("missing Inbox control {label}"))
            };
            let show = rect("Show unimportant");
            let unread = rect("Unread only");
            let accounts = rect(account_label);
            let refresh = rect("↻");
            assert!((show.center().y - unread.center().y).abs() < 2.0);
            assert!((show.center().y - refresh.center().y).abs() < 3.0);
            assert!((show.center().y - accounts.center().y).abs() < 3.0);
            assert!(
                show.right() < unread.left()
                    && unread.right() < accounts.left()
                    && accounts.right() < refresh.left(),
                "{show:?} {unread:?} {accounts:?} {refresh:?}"
            );
            assert!(refresh.right() <= right_edge);
        }
    }

    #[test]
    fn color_scheme_expands_and_can_be_selected_in_the_settings_menu() {
        let ctx = egui::Context::default();
        let path = std::env::temp_dir().join(format!(
            "chord-color-scheme-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let mut gui = Gui::with_settings_path(App::new(None, true), &ctx, path.clone());
        let mut time = 0.0;
        let mut frame = |gui: &mut Gui, events| {
            time += 0.25;
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 650.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    gui.sidebar(ctx, gui.theme.palette(ctx.theme()));
                },
            )
        };
        let find_label = |frame: &egui::FullOutput, label| {
            frame
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape
                        && text.galley.job.text == label
                    {
                        return Some(text.pos + text.galley.rect.center().to_vec2());
                    }
                    None
                })
                .unwrap_or_else(|| panic!("missing menu label {label}"))
        };
        let click = |pos| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ]
        };
        let initial = frame(&mut gui, vec![]);
        frame(&mut gui, click(find_label(&initial, "⚙")));
        let settings = frame(&mut gui, vec![]);
        for label in [
            "Settings",
            "Appearance",
            "Display mode",
            "Font style",
            "Service icons",
            "Manage accounts",
        ] {
            find_label(&settings, label);
        }
        frame(&mut gui, click(find_label(&settings, "Color scheme")));
        let expanded = frame(&mut gui, vec![]);
        frame(&mut gui, click(find_label(&expanded, "Nord")));
        assert_eq!(gui.theme.scheme, ColorScheme::Nord);
        let settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(settings["appearance"]["scheme"], "nord");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn icon_style_saves_and_reloads_with_theme_and_pins() {
        let ctx = egui::Context::default();
        let path = std::env::temp_dir().join(format!(
            "chord-icon-style-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let mut gui = Gui::with_settings_path(App::new(None, true), &ctx, path.clone());
        assert_eq!(gui.icon_style, icons::IconStyle::Brand);
        gui.icon_style = icons::IconStyle::Original;
        gui.theme = Theme {
            mode: Some(DisplayMode::Light),
            scheme: ColorScheme::Solarized,
            font: FontStyle::Terminal,
        };
        let id = gui.app.conversations[0].id.clone();
        gui.app.move_pin(&id, Some(0));
        gui.save_settings();

        let restored = Gui::with_settings_path(App::new(None, true), &ctx, path.clone());
        assert_eq!(restored.icon_style, icons::IconStyle::Original);
        assert_eq!(restored.theme, gui.theme);
        assert!(restored.app.is_pinned(&id));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn authoritative_reaction_count_includes_users_missing_from_matrix() {
        use crate::model::ReactionCount;
        let message = Message {
            reactions: vec![
                json!({"reactionKey":"👍","participantID":"one","participantName":"One"}),
                json!({"reactionKey":"👍","participantID":"two","participantName":"Two"}),
            ],
            reaction_counts: Some(vec![ReactionCount {
                key: "👍".into(),
                count: 4,
                ..Default::default()
            }]),
            ..Default::default()
        };
        let groups = message_reaction_groups(&message, "");
        assert_eq!(groups[0].count, 4);
        assert_eq!(groups[0].names, ["One", "Two"]);
        assert_eq!(groups[0].unnamed, 2);
        let cleared = Message {
            reaction_counts: Some(vec![]),
            ..message
        };
        assert!(message_reaction_groups(&cleared, "").is_empty());
    }

    #[test]
    fn reaction_list_names_everyone_and_puts_you_first() {
        let reactions = vec![
            json!({"reactionKey":"👍","participantID":"alex","participantName":"Alex Chen"}),
            json!({"reactionKey":"👍","participantID":"alex","participantName":"Alex Chen"}),
            json!({"reactionKey":"👍","participantID":"@max:chord.local","participantName":"Max"}),
            json!({"reactionKey":"👍","participantID":"@discord_max:chord.local","participantName":"You"}),
            json!({"reactionKey":"👍","participantID":"local-you","participantName":"You","local":true}),
            json!({"reactionKey":"👍","participantID":"sam","participantName":"Sam"}),
            json!({"reactionKey":"❤️","participantID":"unknown"}),
            json!({"reactionKey":"mxc://chord.local/blob","shortcode":":blobcat:","participantID":"pat","participantName":"Pat"}),
        ];
        let grouped = reaction_groups(&reactions, "@max:chord.local");
        let thumbs = grouped.iter().find(|group| group.key == "👍").unwrap();
        assert_eq!(thumbs.count, 3);
        assert!(thumbs.mine);
        assert_eq!(thumbs.names, ["You", "Alex Chen", "Sam"]);
        let heart = grouped.iter().find(|group| group.key == "❤️").unwrap();
        assert_eq!(heart.count, 1);
        assert!(!heart.mine);
        assert!(heart.names.is_empty());
        assert_eq!(heart.unnamed, 1);
        let custom = grouped
            .iter()
            .find(|group| group.key == "mxc://chord.local/blob")
            .unwrap();
        assert_eq!(custom.image.as_deref(), Some("mxc://chord.local/blob"));
        assert_eq!(custom.shortcode.as_deref(), Some(":blobcat:"));
        assert_eq!(custom.names, ["Pat"]);
        let mut many = Vec::new();
        for index in 0..40 {
            many.push(json!({"reactionKey":"🎉","participantID":format!("p{index}"),"participantName":format!("Person {index}")}));
        }
        let party = reaction_groups(&many, "")
            .into_iter()
            .find(|group| group.key == "🎉")
            .unwrap();
        assert_eq!(party.count, 40);
        assert_eq!(party.names.len(), 40);
        assert_eq!(party.names[0], "Person 0");
        assert_eq!(party.names[39], "Person 39");
        assert!(party.names.iter().all(|name| !name.contains("more")));
    }
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
                        gui.message(ui, &chat, &message, 0, gui.theme.palette(ctx.theme()));
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
        assert_eq!(initials(""), "");
        assert_eq!(initials("+1 (908) 902-3769"), "");
        assert_eq!(initials("Mom ❤️"), "M");
        assert_eq!(initials("Aiden, Ronav & 2 more"), "AR");
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
                            gui.message(ui, &chat, &message, 0, gui.theme.palette(ctx.theme()));
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
            if let Some(target) = gui.reaction_target {
                assert_eq!(target.chat.id, chat.id);
                assert_eq!(target.message.id, message.id);
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
                    let p = gui.theme.palette(ctx.theme());
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
    fn unread_opening_scrolls_to_the_divider_instead_of_sticking_to_latest() {
        let ctx = egui::Context::default();
        let mut gui = Gui::new(App::new(None, true), &ctx);
        gui.app.messages = (0..20)
            .map(|i| Message {
                id: format!("m{i}"),
                text: format!("Message {i}"),
                sender_name: "Sam".into(),
                timestamp: "2026-11-11T10:00:00Z".into(),
                ..Default::default()
            })
            .collect();
        gui.app.first_unread = Some("m5".into());
        let mut divider_y = None;
        for frame in 0..5 {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1240., 820.),
                    )),
                    time: Some(frame as f64),
                    ..Default::default()
                },
                |ctx| gui.thread(ctx, gui.theme.palette(ctx.theme())),
            );
            divider_y = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "New messages" => {
                    Some(text.pos.y)
                }
                _ => None,
            });
        }
        let y = divider_y.expect("unread divider is rendered");
        assert!(
            (50.0..200.0).contains(&y),
            "divider should be near the top, got {y}"
        );
        assert!(gui.scroll_to.is_none(), "jump is consumed once");
    }
    #[test]
    fn right_clicking_a_channel_offers_importance_and_five_mute_durations() {
        let ctx = egui::Context::default();
        let mut gui = Gui::new(App::new(None, true), &ctx);
        let chat = gui.app.conversations[0].clone();
        let mut time = 0.0;
        let mut rect = egui::Rect::NOTHING;
        let mut opened = false;
        let mut frame = |gui: &mut Gui, events| {
            time += 0.08;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(480., 420.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response =
                            gui.conversation_row(ui, &chat, gui.theme.palette(ctx.theme()));
                        opened = response.context_menu_opened();
                        rect = response.rect;
                    });
                },
            );
            rect.center()
        };
        let pos = frame(&mut gui, vec![]);
        frame(&mut gui, vec![egui::Event::PointerMoved(pos)]);
        frame(
            &mut gui,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        frame(
            &mut gui,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert!(opened, "right-click should open the channel menu");
        let direct = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                gui.channel_rules(ui, &chat);
            });
        });
        fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(painted) => out.push(painted.galley.job.text.clone()),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        texts(shape, out);
                    }
                }
                _ => {}
            }
        }
        let mut found = Vec::new();
        for shape in &direct.shapes {
            texts(&shape.shape, &mut found);
        }
        let text = found.join("\n");
        for label in [
            "Mark as unimportant",
            "Mute for 1 hour",
            "Mute for 4 hours",
            "Mute for 24 hours",
            "Mute for 3 days",
            "Mute for 7 days",
        ] {
            assert!(text.contains(label), "missing {label} in {text}");
        }
        assert_eq!(text.matches("Mute for").count(), 5, "{text}");
        assert!(!text.contains("Automatic"), "{text}");
    }
    #[test]
    fn muted_row_is_labeled_and_has_no_unread_dot() {
        let ctx = egui::Context::default();
        let mut gui = Gui::new(App::new(None, true), &ctx);
        let mut chat = gui.app.conversations[0].clone();
        chat.unread = true;
        chat.mute_until = Some(chrono::Utc::now().timestamp_millis() + 60_000);
        chat.title = "snipes".into();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                gui.conversation_row(ui, &chat, gui.theme.palette(ctx.theme()));
            });
        });
        fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(painted) => out.push(painted.galley.job.text.clone()),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        texts(shape, out);
                    }
                }
                _ => {}
            }
        }
        let mut found = Vec::new();
        for shape in &output.shapes {
            texts(&shape.shape, &mut found);
        }
        let text = found.join("\n");
        assert!(text.contains("Muted ·"), "{text}");
        gui.app.conversations[0] = chat.clone();
        gui.app.apply_channel_rule(&chat.id, None, Some(Some(60)));
        assert!(!gui.app.conversations[0].unread);
        assert!(gui.app.conversations[0].mute_until.is_some());
    }
    #[test]
    fn dragging_a_chat_to_the_top_pins_it_and_dragging_it_back_down_unpins_it() {
        type Titles = Vec<(String, egui::Pos2)>;
        let ctx = egui::Context::default();
        let mut gui = Gui::new(App::new(None, true), &ctx);
        let settings = std::env::temp_dir().join(format!("chord-pins-{}.json", std::process::id()));
        gui.settings_path = settings.clone();
        let mut time = 0.;
        let mut frame = |gui: &mut Gui, events: Vec<egui::Event>| {
            time += 0.05;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 900.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| gui.inbox(ctx, gui.theme.palette(ctx.theme())),
            );
            let mut titles = Vec::new();
            for shape in output.shapes {
                if let egui::epaint::Shape::Text(text) = shape.shape {
                    titles.push((
                        text.galley.job.text.clone(),
                        text.pos + text.galley.rect.center().to_vec2(),
                    ));
                }
            }
            titles
        };
        let find = |titles: &[(String, egui::Pos2)], title: &str| {
            titles
                .iter()
                .find(|(t, _)| t.starts_with(&format!("{title} · ")))
                .map(|(_, pos)| *pos)
                .unwrap_or_else(|| panic!("{title:?} not in {titles:?}"))
        };
        let drag = |gui: &mut Gui,
                    frame: &mut dyn FnMut(&mut Gui, Vec<egui::Event>) -> Titles,
                    from: egui::Pos2,
                    to: egui::Pos2| {
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(gui, vec![egui::Event::PointerMoved(from)]);
            frame(gui, vec![button(from, true)]);
            for step in 1..=8 {
                frame(
                    gui,
                    vec![egui::Event::PointerMoved(
                        from + (to - from) * (step as f32 / 8.),
                    )],
                );
            }
            frame(gui, vec![button(to, false)]);
            frame(gui, vec![]);
        };
        frame(&mut gui, vec![]);
        let titles = frame(&mut gui, vec![]);
        let chats: Vec<Conversation> = gui.app.visible().into_iter().cloned().collect();
        let (first, third) = (
            find(&titles, &chats[0].title),
            find(&titles, &chats[3].title),
        );
        drag(&mut gui, &mut frame, third, first - egui::vec2(0., 12.));
        assert_eq!(gui.app.visible()[0].id, chats[3].id);
        assert!(gui.app.is_pinned(&chats[3].id));
        assert!(
            gui.app
                .conversations
                .iter()
                .any(|c| c.id == chats[3].id && c.local_priority == chats[3].local_priority)
        );
        assert!(
            std::fs::read_to_string(&settings)
                .unwrap()
                .contains(&chats[3].id)
        );
        let titles = frame(&mut gui, vec![]);
        let (pinned, low) = (
            find(&titles, &chats[3].title),
            find(&titles, &chats[2].title),
        );
        drag(&mut gui, &mut frame, pinned, low + egui::vec2(0., 20.));
        assert!(!gui.app.is_pinned(&chats[3].id));
        assert_eq!(
            gui.app
                .visible()
                .iter()
                .map(|c| c.id.clone())
                .collect::<Vec<_>>(),
            chats.iter().map(|c| c.id.clone()).collect::<Vec<_>>()
        );
        let _ = std::fs::remove_file(settings);
    }
    #[test]
    fn sidebar_rows_are_colored_by_account_and_slack_workspace() {
        let ctx = egui::Context::default();
        let mut gui = Gui::new(App::new(None, true), &ctx);
        let mut lab = gui.app.conversations[0].clone();
        lab.id = "lab".into();
        lab.source = "slack".into();
        lab.direct = false;
        lab.title = "#general".into();
        lab.account_key = "slack:TEMMBGZU3".into();
        lab.account_name = Some("Garcia-Bosch Lab".into());
        lab.server_name = None;
        lab.unread = true;
        let mut course = lab.clone();
        course.id = "course".into();
        course.title = "#announcements".into();
        course.account_key = "slack:TSAU3JLKZ".into();
        course.account_name = Some("15-112 Staff Fall 2026".into());
        course.unread = false;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(420., 400.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let p = gui.theme.palette(ctx.theme());
                    gui.conversation_row(ui, &lab, p);
                    gui.conversation_row(ui, &course, p);
                });
            },
        );
        struct Mark {
            center: egui::Pos2,
            radius: f32,
            fill: Color32,
            stroke: Color32,
            stroke_width: f32,
        }
        fn walk(shape: &egui::Shape, marks: &mut Vec<Mark>, labels: &mut Vec<String>) {
            match shape {
                egui::Shape::Circle(circle) => marks.push(Mark {
                    center: circle.center,
                    radius: circle.radius,
                    fill: circle.fill,
                    stroke: circle.stroke.color,
                    stroke_width: circle.stroke.width,
                }),
                egui::Shape::Text(text) => labels.push(text.galley.job.text.clone()),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, marks, labels);
                    }
                }
                _ => {}
            }
        }
        let mut marks = Vec::new();
        let mut labels = Vec::new();
        for shape in &output.shapes {
            walk(&shape.shape, &mut marks, &mut labels);
        }
        let lab_color = theme::account_color("slack:TEMMBGZU3");
        let course_color = theme::account_color("slack:TSAU3JLKZ");
        assert_ne!(lab_color, course_color);
        let ring = |color: Color32| {
            marks
                .iter()
                .any(|mark| mark.stroke == color && mark.stroke_width >= 2. && mark.radius >= 10.)
        };
        assert!(ring(lab_color), "lab ring missing");
        assert!(
            course_color != lab_color && ring(course_color),
            "course ring missing"
        );
        let dots: Vec<_> = marks
            .iter()
            .filter(|mark| mark.fill == gui.theme.palette(ctx.theme()).blue && mark.radius >= 6.)
            .collect();
        assert_eq!(dots.len(), 1, "unread dot should be the one large mark");
        let lab_ring = marks
            .iter()
            .find(|mark| mark.stroke == lab_color && mark.stroke_width >= 2.)
            .expect("lab ring");
        assert!(
            dots[0].center.x < lab_ring.center.x,
            "unread dot belongs left of the profile"
        );
        let text = labels.join("\n");
        assert!(text.contains("#general · Garcia-Bosch Lab"), "{text}");
        assert!(
            text.contains("#announcements · 15-112 Staff Fall 2026"),
            "{text}"
        );
    }
    #[test]
    fn message_toolbar_fits_and_keeps_actions_clickable() {
        for (width, index, expected_width) in [(760., 0, 177.), (300., 1, 118.)] {
            let ctx = egui::Context::default();
            let mut gui = Gui::new(App::new(None, true), &ctx);
            let chat = gui.app.active.clone().unwrap();
            let message = gui.app.messages[index].clone();
            let mut pane_right = 0.;
            let mut frame = |events| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 300.),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            pane_right = ui.available_rect_before_wrap().right();
                            gui.message(ui, &chat, &message, index, gui.theme.palette(ctx.theme()));
                        });
                    },
                )
            };
            let output = frame(vec![egui::Event::PointerMoved(egui::pos2(80., 28.))]);
            let toolbar = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.corner_radius == egui::CornerRadius::same(8)
                            && rect.stroke.width == 1. =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("hover toolbar");
            assert!(
                (toolbar.width() - expected_width).abs() < 0.5,
                "{toolbar:?}"
            );
            assert!((toolbar.height() - 34.).abs() < 0.5, "{toolbar:?}");

            let more = egui::pos2(toolbar.right() - 17., toolbar.center().y);
            let button = if message.is_sender {
                more - egui::vec2(ACTION_SIZE + ACTION_GAP, 0.)
            } else {
                more
            };
            frame(vec![egui::Event::PointerMoved(button)]);
            for pressed in [true, false] {
                frame(vec![egui::Event::PointerButton {
                    pos: button,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            if message.is_sender {
                assert_eq!(gui.app.edit_message.as_deref(), Some(message.id.as_str()));
                assert_eq!(gui.app.message_selection, index);
            } else {
                assert!(egui::Popup::is_any_open(&ctx));
            }
            assert!(
                (toolbar.right() - (pane_right - 10.)).abs() < 0.5,
                "{toolbar:?}"
            );
        }
    }

    #[test]
    fn hovered_message_shortcuts_target_the_message_and_respect_text_focus() {
        use egui::Key::{E, R};
        for (key, index, typing, modifiers, activates) in [
            (R, 0, false, egui::Modifiers::NONE, true),
            (E, 1, false, egui::Modifiers::NONE, true),
            (E, 0, false, egui::Modifiers::NONE, false),
            (R, 0, true, egui::Modifiers::NONE, false),
            (E, 1, true, egui::Modifiers::NONE, false),
            (E, 1, false, egui::Modifiers::SHIFT, false),
        ] {
            let ctx = egui::Context::default();
            let mut gui = Gui::new(App::new(None, true), &ctx);
            let chat = gui.app.active.clone().unwrap();
            let message = gui.app.messages[index].clone();
            let id = composer_id(&chat);
            let draw = |gui: &mut Gui, events, focus| {
                let _ = ctx.run(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let composer =
                                ui.add(egui::TextEdit::singleline(&mut gui.app.editor.text).id(id));
                            if focus {
                                composer.request_focus();
                            }
                            gui.message(ui, &chat, &message, index, gui.theme.palette(ctx.theme()));
                        });
                    },
                );
            };
            draw(&mut gui, vec![], typing);
            let letter = if key == R { "r" } else { "e" };
            draw(
                &mut gui,
                vec![
                    egui::Event::PointerMoved(egui::pos2(100., 50.)),
                    egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    },
                    egui::Event::Text(letter.into()),
                ],
                false,
            );
            assert_eq!(
                gui.app.reply.as_ref().map(|reply| &reply.id),
                (activates && key == R).then_some(&message.id)
            );
            assert_eq!(
                gui.app.edit_message.as_ref(),
                (activates && key == E).then_some(&message.id)
            );
            if activates {
                assert!(ctx.memory(|memory| memory.has_focus(id)));
                assert_eq!(
                    gui.app.editor.text,
                    if key == E { &message.text } else { "" }
                );
            } else if typing {
                assert_eq!(gui.app.editor.text, letter);
            }
        }
    }

    #[test]
    fn composer_completes_emoji_and_mentions_without_sending() {
        for (query, key, expected) in [
            (":thumbsup", egui::Key::Tab, "👍 "),
            (":flag_mm", egui::Key::Enter, "🇲🇲 "),
            (":my_custom", egui::Key::Enter, ":my_custom: "),
            (":archived", egui::Key::Tab, ":archived: "),
            ("@vict", egui::Key::Tab, "@Victoria "),
            ("@vicky", egui::Key::Tab, "@Victoria "),
        ] {
            let ctx = egui::Context::default();
            let mut gui = Gui::new(App::new(None, true), &ctx);
            let mut chat = gui.app.active.clone().unwrap();
            chat.source = "discord".into();
            chat.server_id = Some("guild".into());
            gui.app.messages[0].sender_id = Some("@discord_123:chord.local".into());
            gui.mention_members.insert(
                chat.id.clone(),
                (
                    std::time::Instant::now(),
                    vec![MentionMember {
                        id: "@discord_123:chord.local".into(),
                        full_name: "Victoria".into(),
                        aliases: vec!["vicky".into()],
                        is_self: false,
                    }],
                ),
            );
            gui.room_custom_emoji.insert(
                chat.id.clone(),
                vec![RoomEmoji {
                    name: "archived".into(),
                    mxc: Some("mxc://chord.local/archived".into()),
                    thumbnail: false,
                }],
            );
            gui.discord_emoji.insert(
                "guild".into(),
                Some(Ok(vec![DiscordEmoji {
                    id: "123456789012345678".into(),
                    name: "my_custom".into(),
                    mxc: None,
                }])),
            );
            let run = |gui: &mut Gui, events, focus| {
                let _ = ctx.run(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            if focus {
                                ui.memory_mut(|memory| memory.request_focus(composer_id(&chat)));
                            }
                            gui.composer_completion_open = false;
                            gui.composer(ui, &chat, gui.theme.palette(ctx.theme()));
                        });
                    },
                );
            };
            run(&mut gui, vec![], true);
            run(&mut gui, vec![egui::Event::Text(query.into())], false);
            assert!(gui.composer_completion_open);
            run(
                &mut gui,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                false,
            );
            assert_eq!(gui.app.editor.text, expected);
            assert!(!gui.composer_completion_open);
        }
    }

    #[test]
    fn reply_preview_keeps_cancel_inside_the_pane_and_escape_preserves_the_draft() {
        let ctx = egui::Context::default();
        let mut gui = Gui::new(App::new(None, true), &ctx);
        let chat = gui.app.active.clone().unwrap();
        let reply = Message {
            sender_name: "Long sender name ".repeat(15),
            text: "Long reply preview ".repeat(60),
            ..Default::default()
        };
        gui.app.reply = Some(reply.clone());
        gui.app.editor.set("my draft".into());
        gui.app.mode = Mode::Compose;
        let frame = |gui: &mut Gui, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(280., 220.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    gui.handle_escape(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        gui.composer(ui, &chat, gui.theme.palette(ctx.theme()));
                    });
                },
            )
        };
        let output = frame(&mut gui, vec![]);
        let close = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "×" => {
                    Some(text.pos + text.galley.size() / 2.)
                }
                _ => None,
            })
            .expect("reply close button");
        assert!((240. ..280.).contains(&close.x), "{close:?}");
        frame(&mut gui, vec![egui::Event::PointerMoved(close)]);
        for pressed in [true, false] {
            frame(
                &mut gui,
                vec![egui::Event::PointerButton {
                    pos: close,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert!(gui.app.reply.is_none());
        assert_eq!(gui.app.editor.text, "my draft");

        gui.app.reply = Some(reply);
        frame(
            &mut gui,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(gui.app.reply.is_none());
        assert_eq!(gui.app.editor.text, "my draft");
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
                    let p = gui.theme.palette(ctx.theme());
                    gui.sidebar(ctx, p);
                    gui.inbox(ctx, p);
                    gui.thread(ctx, p);
                },
            );
            assert!(!output.shapes.is_empty());
        }
    }
}
