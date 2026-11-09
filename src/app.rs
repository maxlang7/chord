use crate::{
    api::Api,
    model::{Conversation, Inbox, Message, SOURCES, clean},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};
use std::{collections::HashMap, time::Instant};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Inbox,
    Messages,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Browse,
    Search,
    Compose,
    React,
    Confirm,
    Accounts,
    Login,
    Help,
    Attach,
    Details,
    Contacts,
    ContactName,
    ContactLink,
    ConfirmContactLink,
    Servers,
    Image,
}

#[derive(Default)]
pub struct Editor {
    pub text: String,
    pub cursor: usize,
}
impl Editor {
    pub fn set(&mut self, s: String) {
        self.cursor = s.len();
        self.text = s;
    }
    pub fn insert(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }
    pub fn key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert(&c.to_string())
            }
            KeyCode::Enter => self.insert("\n"),
            KeyCode::Backspace if self.cursor > 0 => {
                let n = self.text[..self.cursor]
                    .grapheme_indices(true)
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                self.text.drain(n..self.cursor);
                self.cursor = n
            }
            KeyCode::Delete if self.cursor < self.text.len() => {
                let n = self.text[self.cursor..]
                    .graphemes(true)
                    .next()
                    .map(str::len)
                    .unwrap_or(0);
                self.text.drain(self.cursor..self.cursor + n);
            }
            KeyCode::Left => {
                self.cursor = self.text[..self.cursor]
                    .grapheme_indices(true)
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            }
            KeyCode::Right if self.cursor < self.text.len() => {
                self.cursor += self.text[self.cursor..]
                    .graphemes(true)
                    .next()
                    .map(str::len)
                    .unwrap_or(0)
            }
            KeyCode::Home => {
                self.cursor = self.text[..self.cursor]
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0)
            }
            KeyCode::End => {
                self.cursor = self.text[self.cursor..]
                    .find('\n')
                    .map(|i| self.cursor + i)
                    .unwrap_or(self.text.len())
            }
            _ => {}
        }
    }
}

pub enum Response {
    Image(String, Result<crate::images::Decoded, String>),
    Inbox(u64, Result<Value, String>, bool),
    Messages(String, u64, Result<Value, String>, bool, bool),
    Operation(String, Result<Value, String>),
    ReadState(String, bool, Result<Value, String>),
    Sent(String, Result<Value, String>),
    Uploaded(String, Result<Value, String>),
    Accounts(Result<Value, String>),
    Login(String, Result<Value, String>),
}
fn result(r: anyhow::Result<Value>) -> Result<Value, String> {
    r.map_err(|e| format!("{e:#}"))
}

#[derive(Clone, Default)]
struct Draft {
    text: String,
    reply: Option<Message>,
    attachment: Option<Value>,
    edit_message: Option<String>,
}
#[derive(Clone)]
struct CachedThread {
    messages: Vec<Message>,
    cursor: Option<String>,
    selection: usize,
    used: Instant,
}
pub struct App {
    pub send_busy: bool,
    pub native_ui: bool,
    pub images: crate::images::Images,
    pub api: Option<Api>,
    pub demo: bool,
    pub source: usize,
    pub unread: bool,
    pub conversations: Vec<Conversation>,
    pub messages: Vec<Message>,
    pub selection: usize,
    pub message_selection: usize,
    pub active: Option<Conversation>,
    pub focus: Focus,
    pub mode: Mode,
    pub editor: Editor,
    pub search: String,
    pub reply: Option<Message>,
    pub pending_attachment: Option<Value>,
    pub edit_message: Option<String>,
    pub reaction: usize,
    pub notice: String,
    pub errors: Vec<String>,
    pub loading: bool,
    pub message_loading: bool,
    pub message_cursor: Option<String>,
    message_request_seq: u64,
    message_cache: HashMap<String, CachedThread>,
    pub inbox_cursors: Value,
    pub scroll: u16,
    pub quit: bool,
    pub account_selection: usize,
    pub accounts: Value,
    pub login_id: String,
    pub login: Value,
    pub login_field: usize,
    pub login_values: HashMap<String, String>,
    pub login_busy: bool,
    pub request_seq: u64,
    pub last_refresh: Instant,
    pub last_message_refresh: Instant,
    pub last_login_poll: Instant,
    pub tx: UnboundedSender<Response>,
    pub rx: UnboundedReceiver<Response>,
    drafts: HashMap<String, Draft>,
    pub confirm_body: Option<Value>,
    pub confirm_action: String,
    pub status: Value,
    pub connections: Value,
    pending_read: Option<String>,
    read_tx: Option<UnboundedSender<(String, bool)>>,
    pub archive_status: Value,
    pub last_connection_refresh: Instant,
    pub focus_view: bool,
    pub discord_scope: Option<String>,
    pub server_focus: bool,
    pub server_label: String,
    pub servers: Vec<Value>,
    pub server_query: String,
    pub show_other_servers: bool,
    pub server_selection: usize,
    pub inbox_summary: Value,
    pub contacts: Vec<Value>,
    pub contact_selection: usize,
    pub contact_query: String,
    pub contact_editor: Editor,
    pub contact_conversation: usize,
    pub link_source: Value,
    pub details: Value,
}
pub const REACTIONS: &[&str] = &[
    "👍", "❤️", "😂", "🎉", "👀", "🙏", "✅", "🔥", "💙", "😮", "😢", "💯",
];
impl App {
    pub fn new(api: Option<Api>, demo: bool) -> Self {
        let (tx, rx) = unbounded_channel();
        let read_tx = api.clone().filter(|_| !demo).map(|api| {
            let (queue, mut reads) = unbounded_channel::<(String, bool)>();
            let responses = tx.clone();
            tokio::spawn(async move {
                while let Some((id, read)) = reads.recv().await {
                    let route = Api::conversation(&id, if read { "read" } else { "unread" });
                    let response = result(api.post(&route, json!({})).await);
                    let _ = responses.send(Response::ReadState(id, read, response));
                }
            });
            queue
        });
        let mut app = Self {
            send_busy: false,
            native_ui: false,
            images: crate::images::Images::default(),
            api,
            demo,
            source: 0,
            unread: false,
            conversations: vec![],
            messages: vec![],
            selection: 0,
            message_selection: 0,
            active: None,
            focus: Focus::Inbox,
            mode: Mode::Browse,
            editor: Editor::default(),
            search: String::new(),
            reply: None,
            pending_attachment: None,
            edit_message: None,
            reaction: 0,
            notice: "Connecting to your local accounts…".into(),
            errors: vec![],
            loading: false,
            message_loading: false,
            message_cursor: None,
            message_request_seq: 0,
            message_cache: HashMap::new(),
            inbox_cursors: Value::Null,
            scroll: 0,
            quit: false,
            account_selection: 0,
            accounts: Value::Null,
            login_id: String::new(),
            login: Value::Null,
            login_field: 0,
            login_values: HashMap::new(),
            login_busy: false,
            request_seq: 0,
            last_refresh: Instant::now(),
            last_message_refresh: Instant::now(),
            last_login_poll: Instant::now(),
            tx,
            rx,
            drafts: HashMap::new(),
            confirm_body: None,
            confirm_action: "send".into(),
            status: Value::Null,
            connections: Value::Null,
            pending_read: None,
            read_tx,
            archive_status: Value::Null,
            last_connection_refresh: Instant::now(),
            focus_view: true,
            discord_scope: None,
            server_focus: false,
            server_label: String::new(),
            servers: vec![],
            server_query: String::new(),
            show_other_servers: false,
            server_selection: 0,
            inbox_summary: Value::Null,
            contacts: vec![],
            contact_selection: 0,
            contact_query: String::new(),
            contact_editor: Editor::default(),
            contact_conversation: 0,
            link_source: Value::Null,
            details: Value::Null,
        };
        if demo {
            let (c, m) = crate::model::demo();
            app.active = c.first().cloned();
            app.conversations = c;
            app.messages = m;
            app.notice = "Design preview · network actions are disabled".into();
            app.message_selection = 2
        } else {
            app.refresh(false);
            app.load_status();
            app.load_connections()
        }
        app
    }
    pub fn load_connections(&mut self) {
        self.last_connection_refresh = Instant::now();
        if let Some(api) = self.api.clone() {
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(Response::Operation(
                    "connections".into(),
                    result(api.get("/api/connections").await),
                ));
            });
        }
    }
    fn load_status(&self) {
        if let Some(api) = self.api.clone() {
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let r = api.get("/api/status").await;
                let _ = tx.send(Response::Operation("status".into(), result(r)));
            });
        }
    }
    pub fn visible(&self) -> Vec<&Conversation> {
        self.conversations
            .iter()
            .filter(|c| {
                (self.source == 0 || c.source == SOURCES[self.source].0)
                    && (!self.unread || c.unread)
            })
            .collect()
    }
    pub fn selected(&self) -> Option<Conversation> {
        self.visible().get(self.selection).map(|c| (*c).clone())
    }
    pub fn selected_message(&self) -> Option<Message> {
        self.messages.get(self.message_selection).cloned()
    }
    pub fn focus_enabled(&self) -> bool {
        if self.discord_scope.is_some() {
            self.server_focus
        } else {
            self.focus_view
        }
    }
    pub fn source_title(&self) -> &str {
        if self.source == 2 && self.discord_scope.is_some() {
            &self.server_label
        } else {
            SOURCES[self.source].1
        }
    }
    pub fn visible_servers(&self) -> Vec<Value> {
        let mut entries = vec![
            json!({"id":"dm","name":"Direct messages","description":"Private and group conversations"}),
            json!({"id":"all","name":"All Discord channels","description":"Every imported server and conversation"}),
        ];
        entries.extend(self.servers.clone());
        let query = self.server_query.to_lowercase();
        entries
            .into_iter()
            .filter(|s| {
                (query.is_empty() && (self.show_other_servers || s["preferred"] != false))
                    || (!query.is_empty()
                        && s["name"]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&query))
            })
            .collect()
    }
    pub fn open_servers(&mut self) {
        self.mode = Mode::Servers;
        self.server_query.clear();
        self.server_selection = 0;
        if let Some(api) = self.api.clone() {
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(Response::Operation(
                    "discord_servers".into(),
                    result(api.get("/api/discord/servers").await),
                ));
            });
        }
    }
    fn server_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.show_other_servers = !self.show_other_servers;
                self.server_selection = 0;
            }
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Down => {
                self.server_selection =
                    (self.server_selection + 1).min(self.visible_servers().len().saturating_sub(1))
            }
            KeyCode::Up => self.server_selection = self.server_selection.saturating_sub(1),
            KeyCode::Backspace => {
                self.server_query.pop();
                self.server_selection = 0;
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.server_query.push(c);
                self.server_selection = 0;
            }
            KeyCode::Enter => {
                if let Some(server) = self.visible_servers().get(self.server_selection) {
                    self.discord_scope = server["id"].as_str().map(str::to_owned);
                    self.server_label = server["name"].as_str().unwrap_or("Discord").to_owned();
                    self.source = 2;
                    self.server_focus = false;
                    self.unread = false;
                    self.search.clear();
                    self.selection = 0;
                    self.focus = Focus::Inbox;
                    self.mode = Mode::Browse;
                    self.refresh(false);
                }
            }
            _ => {}
        }
    }
    pub fn visible_contacts(&self) -> Vec<&Value> {
        let query = self.contact_query.to_lowercase();
        self.contacts
            .iter()
            .filter(|p| {
                (self.mode != Mode::ContactLink || p["id"] != self.link_source["id"])
                    && (query.is_empty() || p.to_string().to_lowercase().contains(&query))
            })
            .collect()
    }
    pub fn fetch_contacts(&self) {
        if let Some(api) = self.api.clone() {
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(Response::Operation(
                    "contacts".into(),
                    result(api.get("/api/contacts").await),
                ));
            });
        }
    }
    pub fn open_contacts(&mut self) {
        self.mode = Mode::Contacts;
        self.contact_query.clear();
        self.contact_selection = 0;
        self.fetch_contacts();
    }
    pub fn open_contact(&mut self, contact: Value) {
        self.details = contact.clone();
        self.contact_conversation = 0;
        self.mode = Mode::Details;
        if let (Some(api), Some(id)) = (self.api.clone(), contact["id"].as_str()) {
            let route = format!("/api/contacts/{id}");
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(Response::Operation(
                    "contact_details".into(),
                    result(api.get(&route).await),
                ));
            });
        }
    }
    fn contact_key(&mut self, key: KeyEvent) {
        match self.mode {

            Mode::Contacts | Mode::ContactLink => match key.code {
                KeyCode::Esc => self.mode = if self.mode == Mode::ContactLink { Mode::Details } else { Mode::Browse },
                KeyCode::Down => self.contact_selection = (self.contact_selection + 1).min(self.visible_contacts().len().saturating_sub(1)),
                KeyCode::Up => self.contact_selection = self.contact_selection.saturating_sub(1),
                KeyCode::Enter => {
                    if let Some(person) = self.visible_contacts().get(self.contact_selection).map(|p| (*p).clone()) {
                        if self.mode == Mode::ContactLink { self.details = person; self.mode = Mode::ConfirmContactLink; }
                        else { self.open_contact(person); }
                    }
                }
                KeyCode::Backspace => { self.contact_query.pop(); self.contact_selection = 0; }
                KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                    self.contact_query.push(c); self.contact_selection = 0;
                }
                _ => {}
            },
            Mode::Details => match key.code {
                KeyCode::Esc => { self.mode = Mode::Contacts; self.fetch_contacts(); }
                KeyCode::Char('e') => {
                    self.contact_editor.set(self.details["name"].as_str().unwrap_or("").into());
                    self.mode = Mode::ContactName;
                }
                KeyCode::Char('*') => {
                    if let Some(id) = self.details["id"].as_str() {
                        self.operation("contact_saved", format!("/api/contacts/{id}"), json!({"favorite":self.details["favorite"] != true}));
                    }
                }
                KeyCode::Char('m') => {
                    self.link_source = self.details.clone(); self.contact_query.clear();
                    self.contact_selection = 0; self.mode = Mode::ContactLink; self.fetch_contacts();
                }
                KeyCode::Char('u') => self.operation("contacts_undone", "/api/contacts/undo-merge".into(), json!({})),
                KeyCode::Down | KeyCode::Char('j') => self.contact_conversation = (self.contact_conversation + 1).min(self.details["conversations"].as_array().map(Vec::len).unwrap_or(0).saturating_sub(1)),
                KeyCode::Up | KeyCode::Char('k') => self.contact_conversation = self.contact_conversation.saturating_sub(1),
                KeyCode::Enter => {
                    if let Ok(c) = serde_json::from_value::<Conversation>(self.details["conversations"][self.contact_conversation].clone()) {
                        self.mode = Mode::Browse; self.open(c);
                    }
                }
                _ => {}
            },
            Mode::ContactName => match key.code {
                KeyCode::Esc => self.mode = Mode::Details,
                KeyCode::Enter => {
                    if let Some(id) = self.details["id"].as_str() {
                        self.operation("contact_saved", format!("/api/contacts/{id}"), json!({"name":self.contact_editor.text.trim()}));
                    }
                }
                _ => self.contact_editor.key(key),
            },
            Mode::ConfirmContactLink => match key.code {
                KeyCode::Esc => { self.details = self.link_source.clone(); self.mode = Mode::Details; }
                KeyCode::Enter => self.operation("contact_saved", "/api/contacts/merge".into(), json!({"sourceID":self.link_source["id"],"targetID":self.details["id"],"confirm":true})),
                _ => {}
            },
            _ => {}
        }
    }
    pub fn refresh(&mut self, append: bool) {
        self.refresh_inbox(append, false);
    }
    pub fn refresh_now(&mut self) {
        self.refresh_inbox(false, true);
        self.request_messages(false, !self.messages.is_empty(), true);
        self.load_connections();
    }
    fn refresh_inbox(&mut self, append: bool, fresh: bool) {
        if self.demo {
            return;
        }
        self.request_seq += 1;
        let seq = self.request_seq;
        self.loading = true;
        self.last_refresh = Instant::now();
        if let Some(api) = self.api.clone() {
            let api = if fresh { api.fresh() } else { api };
            let source = SOURCES[self.source].0.to_string();
            let query = self.search.clone();
            let view = if self.focus_enabled() && query.is_empty() {
                "focus"
            } else {
                "all"
            };
            let unread = self.unread;
            let server = self.discord_scope.clone();
            let cursor = if append {
                self.inbox_cursors
                    .get(&source)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            } else {
                None
            };
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let r = api
                    .inbox(
                        &source,
                        view,
                        &query,
                        unread,
                        cursor.as_deref(),
                        server.as_deref(),
                    )
                    .await;
                let refreshing = r.as_ref().is_ok_and(|v| v["_cache"]["refreshing"] == true);
                let _ = tx.send(Response::Inbox(seq, result(r), append));
                if refreshing {
                    let r = api
                        .fresh()
                        .inbox(
                            &source,
                            view,
                            &query,
                            unread,
                            cursor.as_deref(),
                            server.as_deref(),
                        )
                        .await;
                    let _ = tx.send(Response::Inbox(seq, result(r), append));
                }
            });
        }
    }
    fn remember_messages(&mut self) {
        if self.messages.is_empty() {
            return;
        }
        if let Some(c) = &self.active {
            self.message_cache.insert(
                c.id.clone(),
                CachedThread {
                    messages: self.messages.clone(),
                    cursor: self.message_cursor.clone(),
                    selection: self.message_selection,
                    used: Instant::now(),
                },
            );
        }
        if self.message_cache.len() > 256
            && let Some(oldest) = self
                .message_cache
                .iter()
                .min_by_key(|(_, c)| c.used)
                .map(|(id, _)| id.clone())
        {
            self.message_cache.remove(&oldest);
        }
    }
    pub fn open(&mut self, c: Conversation) {
        self.remember_messages();
        let cached = self.message_cache.get(&c.id).cloned();
        if let Some(old) = &self.active {
            let draft = Draft {
                text: self.editor.text.clone(),
                reply: self.reply.clone(),
                attachment: self.pending_attachment.clone(),
                edit_message: self.edit_message.clone(),
            };
            if draft.text.is_empty()
                && draft.reply.is_none()
                && draft.attachment.is_none()
                && draft.edit_message.is_none()
            {
                self.drafts.remove(&old.id);
            } else {
                self.drafts.insert(old.id.clone(), draft);
            }
        }
        let draft = self.drafts.get(&c.id).cloned().unwrap_or_default();
        self.editor = Editor::default();
        self.editor.set(draft.text);
        self.reply = draft.reply;
        self.edit_message = draft.edit_message;
        self.pending_attachment = draft.attachment;
        self.pending_read =
            (c.kind != "archive" && c.thread_root_id.is_none()).then(|| c.id.clone());
        self.active = Some(c);
        self.focus = Focus::Messages;
        self.messages.clear();
        self.message_cursor = None;
        self.message_selection = 0;
        if let Some(cached) = cached {
            self.messages = cached.messages;
            self.message_cursor = cached.cursor;
            self.message_selection = cached.selection;
            self.notice = "Showing saved messages. Updating…".into();
        }
        self.scroll = 0;
        if !self.messages.is_empty() && self.pending_read.is_some() {
            self.set_read_state(true);
        }
        self.load_messages(false);
    }
    pub fn open_thread(&mut self, message: &Message) {
        let Some(mut chat) = self.active.clone() else {
            return;
        };
        if chat.kind != "chat" {
            return;
        }
        let root = message
            .thread_root_id
            .clone()
            .unwrap_or_else(|| message.id.clone());
        let parent = chat
            .parent_conversation_id
            .clone()
            .unwrap_or_else(|| chat.id.clone());
        chat.id = format!(
            "matrix-thread:{}:{}",
            urlencoding::encode(&chat.remote_id),
            urlencoding::encode(&root)
        );
        chat.thread_root_id = Some(root);
        chat.parent_conversation_id = Some(parent);
        self.open(chat);
    }
    pub fn back_to_channel(&mut self) {
        let Some(mut chat) = self.active.clone() else {
            return;
        };
        let Some(parent) = chat.parent_conversation_id.take() else {
            return;
        };
        chat.id = parent;
        chat.thread_root_id = None;
        self.open(chat);
    }
    pub fn set_read_state(&mut self, read: bool) {
        self.pending_read = None;
        if let Some(c) = &self.active
            && c.kind != "archive"
            && let Some(queue) = &self.read_tx
        {
            let _ = queue.send((c.id.clone(), read));
        }
    }
    pub fn load_messages(&mut self, older: bool) {
        self.request_messages(older, !older && !self.messages.is_empty(), false);
    }
    fn request_messages(&mut self, older: bool, preserve: bool, fresh: bool) {
        if self.demo {
            return;
        }
        let Some(c) = self.active.clone() else { return };
        self.message_request_seq += 1;
        let seq = self.message_request_seq;
        self.message_loading = true;
        self.last_message_refresh = Instant::now();
        if let Some(api) = self.api.clone() {
            let api = if fresh { api.fresh() } else { api };
            let tx = self.tx.clone();
            let cursor = if older {
                self.message_cursor.clone()
            } else {
                None
            };
            tokio::spawn(async move {
                let r = api.messages(&c.id, cursor.as_deref()).await;
                let refreshing = r.as_ref().is_ok_and(|v| v["_cache"]["refreshing"] == true);
                let _ = tx.send(Response::Messages(
                    c.id.clone(),
                    seq,
                    result(r),
                    older,
                    preserve,
                ));
                if refreshing {
                    let r = api.fresh().messages(&c.id, cursor.as_deref()).await;
                    let _ = tx.send(Response::Messages(c.id, seq, result(r), older, true));
                }
            });
        }
    }
    pub fn operation(&self, label: &str, route: String, body: Value) {
        if self.demo {
            return;
        }
        if let Some(api) = self.api.clone() {
            let tx = self.tx.clone();
            let label = label.to_owned();
            tokio::spawn(async move {
                let r = api.post(&route, body).await;
                let _ = tx.send(Response::Operation(label, result(r)));
            });
        }
    }
    pub fn begin_compose(&mut self, reply: bool) {
        let Some(c) = self.active.as_ref() else {
            return;
        };
        if !c.can_send {
            self.notice = "This connection cannot send yet. Open accounts with a.".into();
            return;
        }
        if reply {
            if c.kind == "phone" {
                self.notice="BlueFerry can send to this conversation. Native replies and emoji reactions are unavailable.".into();
                return;
            }
            self.reply = self.selected_message();
        }
        self.mode = Mode::Compose;
    }
    pub fn prepare_send(&mut self) {
        if self.send_busy {
            return;
        }
        if self.editor.text.trim().is_empty() && self.pending_attachment.is_none() {
            return;
        }
        let Some(c) = self.active.clone() else { return };
        if self.reply.is_some() && self.pending_attachment.is_some() && c.kind == "chat" {
            self.notice = "Send the text reply and attachment separately. This bridge cannot attach a native reply to media yet.".into();
            return;
        }
        let mut body = json!({"text":self.editor.text});
        if c.kind == "email" {
            let Some(m) = self.reply.clone().or_else(|| self.messages.last().cloned()) else {
                return;
            };
            body["to"] = json!(if m.is_sender {
                m.to.clone()
            } else {
                m.from.clone()
            });
            body["subject"] = json!(if m.subject.to_lowercase().starts_with("re:") {
                m.subject.clone()
            } else {
                format!("Re: {}", m.subject)
            });
            body["replyToMessageId"] = json!(m.message_id_header);
            body["references"] = json!(format!("{} {}", m.references, m.message_id_header).trim());
            body["confirmSend"] = json!(true);
        } else if c.kind == "phone" {
            if c.blue["is_group"].as_bool().unwrap_or(false) {
                body["confirmGroup"] = json!(true);
                body["expectedGroupToken"] = c.blue["confirmation_token"].clone();
            }
        } else {
            if let Some(reply) = &self.reply {
                body["replyToMessageID"] = json!(reply.id)
            }
            if let Some(a) = &self.pending_attachment {
                body["attachment"] = a.clone()
            }
        }
        self.confirm_body = Some(body);
        self.confirm_action = if self.edit_message.is_some() {
            "edit"
        } else {
            "send"
        }
        .into();
        self.mode = Mode::Confirm;
    }
    pub fn confirm(&mut self) {
        if self.send_busy {
            return;
        }
        let (Some(c), Some(mut b)) = (self.active.clone(), self.confirm_body.take()) else {
            return;
        };
        if let Some(id) = self.edit_message.clone() {
            b["messageID"] = json!(id)
        }
        self.send_busy = !self.demo;
        if let Some(api) = self.api.clone() {
            let route = Api::conversation(&c.id, &self.confirm_action);
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let r = api.post(&route, b).await;
                let _ = tx.send(Response::Sent(c.id, result(r)));
            });
        }
        self.notice = "Submitting your message…".into();
        self.mode = Mode::Compose;
    }
    pub fn load_archive_status(&mut self) {
        if let Some(api) = self.api.clone() {
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let response = match api.get("/api/archive/status").await {
                    Ok(value) => value,
                    Err(error) => json!({"statusError": error.to_string()}),
                };
                let _ = tx.send(Response::Operation("archive_status".into(), Ok(response)));
            });
        }
    }
    pub fn load_accounts(&mut self) {
        self.load_connections();
        self.load_archive_status();
        self.mode = Mode::Accounts;
        if let Some(api) = self.api.clone() {
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(Response::Accounts(result(api.get("/api/bridges").await)));
            });
        }
    }
    pub fn start_login(&mut self, id: String, flow: Option<&str>) {
        self.login_id = id.clone();
        self.login_busy = true;
        self.mode = Mode::Login;
        self.login = json!({"type":"loading","instructions":"Starting secure sign-in…"});
        self.login_values.clear();
        self.login_field = 0;
        if let Some(api) = self.api.clone() {
            let tx = self.tx.clone();
            let flow = flow.map(str::to_owned);
            tokio::spawn(async move {
                let r = api
                    .post(&format!("/api/bridges/{id}/start"), json!({"flow":flow}))
                    .await;
                let _ = tx.send(Response::Login(id, result(r)));
            });
        }
    }
    pub fn fields(&self) -> Vec<Value> {
        self.login["user_input"]["fields"]
            .as_array()
            .cloned()
            .or_else(|| self.login["cookies"]["fields"].as_array().cloned())
            .unwrap_or_default()
    }
    pub fn submit_login(&mut self) {
        if self.login_busy {
            return;
        }
        if let Some(api) = self.api.clone() {
            let id = self.login_id.clone();
            let tx = self.tx.clone();
            let values = self.login_values.clone();
            self.login_busy = true;
            tokio::spawn(async move {
                let r = api
                    .post(&format!("/api/bridges/{id}/submit"), json!(values))
                    .await;
                let _ = tx.send(Response::Login(id, result(r)));
            });
        }
    }
    pub fn image_targets(&self) -> Vec<crate::images::Target> {
        let Some(chat) = &self.active else {
            return vec![];
        };
        let Some(message) = self.selected_message() else {
            return vec![];
        };
        message
            .attachments
            .iter()
            .enumerate()
            .filter(|(_, a)| crate::images::is_image(a))
            .map(|(index, attachment)| crate::images::Target {
                chat: chat.id.clone(),
                message: message.id.clone(),
                index,
                attachment: attachment.clone(),
            })
            .collect()
    }
    pub fn image_target(&self) -> Option<crate::images::Target> {
        let targets = self.image_targets();
        targets
            .get(self.images.index.min(targets.len().saturating_sub(1)))
            .cloned()
    }
    fn load_image(&mut self) {
        let Some(target) = self.image_target() else {
            return;
        };
        if self.images.message != target.message {
            self.images.index = 0;
            self.images.message = target.message.clone();
        }
        let Some(target) = self.image_target() else {
            return;
        };
        let key = target.key();
        if self.images.cache.contains_key(&key) || self.images.pending.is_some() {
            return;
        }
        let Some(api) = self.api.clone() else {
            return;
        };
        self.images.pending = Some(key.clone());
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let r = async {
                let value = api.post(&Api::conversation(&target.chat, "attachment"), json!({"messageID":target.message,"index":target.index,"attachmentID":target.attachment["id"]})).await?;
                let encoded = value["data"].as_str().ok_or_else(|| anyhow::anyhow!("No attachment data"))?.to_string();
                tokio::task::spawn_blocking(move || crate::images::decode(&encoded)).await?
            }.await;
            let _ = tx.send(Response::Image(
                key,
                r.map_err(|e: anyhow::Error| format!("{e:#}")),
            ));
        });
    }
    fn image_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('o') => self.mode = Mode::Browse,
            KeyCode::Right | KeyCode::Char('l') => {
                self.images.index =
                    (self.images.index + 1).min(self.image_targets().len().saturating_sub(1))
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.images.index = self.images.index.saturating_sub(1)
            }
            KeyCode::Char('R') => {
                if let Some(t) = self.image_target() {
                    self.images.cache.remove(&t.key());
                }
            }
            KeyCode::Char('d') | KeyCode::Char('b') => {
                if let Some(t) = self.image_target()
                    && let Some(Ok(loaded)) = self.images.cache.get(&t.key())
                {
                    match crate::images::save(&loaded.data, t.name()) {
                        Ok(file) => {
                            self.notice = format!("Saved {}", file.display());
                            if key.code == KeyCode::Char('b')
                                && let Err(e) = std::process::Command::new("brave")
                                    .arg(&file)
                                    .stdin(std::process::Stdio::null())
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .spawn()
                            {
                                self.notice = format!("Could not open Brave: {e}");
                            }
                        }
                        Err(e) => self.notice = format!("Could not save image: {e}"),
                    }
                }
            }
            _ => {}
        }
    }
    pub fn poll(&mut self) {
        if !self.native_ui
            && matches!(self.mode, Mode::Browse | Mode::Image)
            && self.focus == Focus::Messages
        {
            self.load_image();
        }
        for _ in 0..32 {
            let Ok(event) = self.rx.try_recv() else {
                break;
            };
            if matches!(&event, Response::Operation(label, _) if label == "send") {
                self.send_busy = false;
            }
            match event {
                Response::Sent(id, r) => {
                    self.send_busy = false;
                    match r {
                        Ok(_) => {
                            self.drafts.remove(&id);
                            if self.active.as_ref().is_some_and(|c| c.id == id) {
                                self.editor = Editor::default();
                                self.reply = None;
                                self.edit_message = None;
                                self.pending_attachment = None;
                                self.mode = Mode::Browse;
                                self.load_messages(false);
                            }
                            self.notice =
                                "Submitted. Checking the conversation for confirmation.".into();
                            self.refresh(false);
                        }
                        Err(e) => self.notice = format!("Send failed. Your text is retained. {e}"),
                    }
                }
                Response::Uploaded(id, r) => {
                    if self.active.as_ref().is_none_or(|c| c.id != id) {
                        match r {
                            Ok(v) => {
                                self.drafts.entry(id).or_default().attachment = Some(
                                    json!({"uploadID":v["uploadID"],"fileName":v["fileName"],"mimeType":v["mimeType"]}),
                                );
                                self.notice =
                                    "Attachment ready in the previous conversation.".into();
                            }
                            Err(e) => self.notice = e,
                        }
                        continue;
                    }
                    match r {
                        Ok(v) => {
                            self.pending_attachment = Some(
                                json!({"uploadID":v["uploadID"],"fileName":v["fileName"],"mimeType":v["mimeType"]}),
                            );
                            self.notice = "Attachment ready".into();
                            self.mode = Mode::Compose;
                        }
                        Err(e) => self.notice = e,
                    }
                }
                Response::Image(key, data) => self.images.insert(key, data),
                Response::Inbox(seq, r, append) => {
                    if seq != self.request_seq {
                        continue;
                    }
                    self.loading = false;
                    match r {
                        Ok(v) => {
                            let cached = v["_cache"]["cached"] == true;
                            let refreshing = v["_cache"]["refreshing"] == true;
                            let cache_error = v["_cache"]["error"].is_string();
                            match serde_json::from_value::<Inbox>(v) {
                                Ok(data) => {
                                    let selected = self.selected().map(|c| c.id);
                                    if append {
                                        for c in data.items {
                                            if !self.conversations.iter().any(|old| old.id == c.id)
                                            {
                                                self.conversations.push(c)
                                            }
                                        }
                                    } else {
                                        self.conversations = data.items
                                    }
                                    self.inbox_cursors = data.cursors;
                                    self.inbox_summary = data.summary;
                                    self.errors = data
                                        .errors
                                        .iter()
                                        .map(|e| {
                                            format!(
                                                "{}: {}",
                                                e["source"].as_str().unwrap_or("account"),
                                                e["error"].as_str().unwrap_or("unavailable")
                                            )
                                        })
                                        .collect();
                                    if let Some(id) = selected {
                                        self.selection = self
                                            .visible()
                                            .iter()
                                            .position(|c| c.id == id)
                                            .unwrap_or(0)
                                    }
                                    self.selection =
                                        self.selection.min(self.visible().len().saturating_sub(1));
                                    self.notice = if cached && cache_error {
                                        "Showing saved inbox. Refresh is offline.".into()
                                    } else if cached && refreshing {
                                        "Showing saved inbox. Updating…".into()
                                    } else if cached {
                                        "Loaded saved inbox".into()
                                    } else if self.errors.is_empty() {
                                        "Synced locally".into()
                                    } else {
                                        format!(
                                            "{} account issue(s) · a to inspect",
                                            self.errors.len()
                                        )
                                    }
                                }
                                Err(e) => self.notice = e.to_string(),
                            }
                        }
                        Err(e) => self.notice = e,
                    }
                }
                Response::Messages(id, seq, r, older, preserve) => {
                    if seq != self.message_request_seq {
                        continue;
                    }
                    if self.active.as_ref().map(|c| &c.id) != Some(&id) {
                        continue;
                    }
                    self.message_loading = false;
                    match r {
                        Ok(v) => {
                            if v["_cache"]["cached"] == true {
                                self.notice = if v["_cache"]["error"].is_string() {
                                    "Showing saved messages. Refresh is offline.".into()
                                } else if v["_cache"]["refreshing"] == true {
                                    "Showing saved messages. Updating…".into()
                                } else {
                                    "Loaded saved messages".into()
                                };
                            } else if self.notice.starts_with("Showing saved")
                                || self.notice == "Loaded saved messages"
                            {
                                self.notice = "Messages updated".into();
                            }
                            if let Ok(mut m) =
                                serde_json::from_value::<Vec<Message>>(v["items"].clone())
                            {
                                m.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                                let selected_id = self.selected_message().map(|m| m.id);
                                let follow_latest =
                                    self.message_selection + 1 >= self.messages.len();
                                if older || preserve {
                                    m.extend(self.messages.clone());
                                    let mut seen = std::collections::HashSet::new();
                                    m.retain(|m| seen.insert(m.id.clone()));
                                }
                                m.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                                self.messages = m;
                                self.message_selection = if preserve && !follow_latest {
                                    selected_id
                                        .and_then(|id| {
                                            self.messages.iter().position(|m| m.id == id)
                                        })
                                        .unwrap_or(0)
                                } else if older {
                                    0
                                } else {
                                    self.messages.len().saturating_sub(1)
                                };
                                if !preserve || self.message_cursor.is_none() {
                                    self.message_cursor =
                                        v["oldestCursor"].as_str().map(str::to_owned);
                                }
                                if let Some(c) = self.active.as_mut()
                                    && v["thread"].is_object()
                                {
                                    c.blue = v["thread"].clone()
                                }
                                self.remember_messages();
                                if self.pending_read.as_ref() == Some(&id) {
                                    self.set_read_state(true);
                                }
                            }
                        }
                        Err(e) => self.notice = e,
                    }
                }
                Response::ReadState(id, read, response) => match response {
                    Ok(_) => {
                        for c in &mut self.conversations {
                            if c.id == id {
                                c.unread = !read;
                            }
                        }
                        if let Some(c) = &mut self.active
                            && c.id == id
                        {
                            c.unread = !read;
                        }
                        self.refresh(true);
                    }
                    Err(error) => self.notice = format!("Could not update read state: {error}"),
                },
                Response::Operation(label, r) => match r {
                    Ok(v) => match label.as_str() {
                        "status" => self.status = v,
                        "connections" => self.connections = v,
                        "archive_status" => self.archive_status = v,
                        "discord_servers" => {
                            self.servers = v["items"].as_array().cloned().unwrap_or_default();
                        }
                        "contacts" => {
                            self.contacts = v["items"].as_array().cloned().unwrap_or_default();
                            self.contact_selection = self
                                .contact_selection
                                .min(self.visible_contacts().len().saturating_sub(1));
                        }
                        "contact_details" | "contact_saved" => {
                            if label == "contact_details" && self.mode != Mode::Details {
                                continue;
                            }
                            self.details = v;
                            self.contact_conversation = 0;
                            self.mode = Mode::Details;
                            if label == "contact_saved" {
                                self.notice = "Contact saved on this laptop".into();
                                self.fetch_contacts();
                                self.open_contact(self.details.clone());
                                self.refresh(false);
                                self.load_messages(false);
                            }
                        }
                        "contacts_undone" => {
                            self.mode = Mode::Contacts;
                            self.fetch_contacts();
                            self.refresh(false);
                            self.load_messages(false);
                            self.notice = "Last contact link undone".into();
                        }

                        _ => {
                            self.notice = label;
                            self.refresh(false);
                            self.load_messages(false)
                        }
                    },
                    Err(e) => self.notice = e,
                },
                Response::Accounts(r) => match r {
                    Ok(v) => self.accounts = v,
                    Err(e) => self.notice = e,
                },
                Response::Login(id, r) => {
                    if self.login_id != id {
                        continue;
                    }
                    self.login_busy = false;
                    match r {
                        Ok(v) => {
                            self.login = v;
                            self.login_field = 0;
                            if self.login["type"] == "complete" {
                                self.notice = format!("{} connected", self.login_id);
                                self.refresh(false);
                                self.operation("refresh", "/api/matrix/refresh".into(), json!({}));
                            }
                        }
                        Err(e) => self.login = json!({"type":"error","instructions":e}),
                    }
                }
            }
        }
        if !self.demo && self.last_connection_refresh.elapsed().as_secs() >= 30 {
            self.load_connections();
        }
        if !self.demo
            && self.last_refresh.elapsed().as_secs() >= 30
            && !self.loading
            && (self.mode == Mode::Browse || self.native_ui)
        {
            self.refresh(false)
        }
        if !self.demo
            && (self.native_ui || self.mode == Mode::Browse)
            && !self.message_loading
            && self.active.is_some()
            && self.last_message_refresh.elapsed().as_secs()
                >= if self.active.as_ref().is_some_and(|c| c.kind == "email") {
                    30
                } else {
                    5
                }
        {
            self.request_messages(false, true, false);
        }
        if self.mode == Mode::Login
            && self.login["type"] == "display_and_wait"
            && !self.login_busy
            && self.last_login_poll.elapsed().as_secs() >= 1
        {
            self.last_login_poll = Instant::now();
            self.login_busy = true;
            if let Some(api) = self.api.clone() {
                let id = self.login_id.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let r = api.get(&format!("/api/bridges/{id}/state")).await;
                    let _ = tx.send(Response::Login(id, result(r)));
                });
            }
        }
    }
    pub fn paste(&mut self, text: &str) {
        let text = clean(text);
        match self.mode {
            Mode::Compose | Mode::Attach => self.editor.insert(&text),
            Mode::Search => self.search.push_str(&text.replace('\n', " ")),
            Mode::Servers => {
                self.server_query.push_str(&text.replace('\n', " "));
                self.server_selection = 0;
            }
            Mode::ContactName => self.contact_editor.insert(&text.replace('\n', " ")),
            Mode::Contacts | Mode::ContactLink => {
                self.contact_query.push_str(&text.replace('\n', " "));
                self.contact_selection = 0;
            }

            Mode::Login => {
                if let Some(f) = self.fields().get(self.login_field) {
                    let id = f["id"].as_str().unwrap_or("value").to_owned();
                    self.login_values.entry(id).or_default().push_str(&text);
                }
            }
            _ => {}
        }
    }
    pub fn key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match self.mode {
            Mode::Image => {
                self.image_key(key);
                return;
            }
            Mode::Servers => {
                self.server_key(key);
                return;
            }
            Mode::Contacts
            | Mode::ContactLink
            | Mode::ContactName
            | Mode::ConfirmContactLink
            | Mode::Details => {
                self.contact_key(key);
                return;
            }
            Mode::Help => {
                self.mode = Mode::Browse;
                return;
            }
            Mode::Confirm => {
                match key.code {
                    KeyCode::Enter | KeyCode::Char('y') => self.confirm(),
                    KeyCode::Esc | KeyCode::Char('n') => {
                        self.confirm_body = None;
                        self.mode = Mode::Compose
                    }
                    _ => {}
                }
                return;
            }
            Mode::Search => {
                match key.code {
                    KeyCode::Esc => {
                        self.search.clear();
                        self.mode = Mode::Browse;
                        self.refresh(false)
                    }
                    KeyCode::Enter => {
                        self.mode = Mode::Browse;
                        self.selection = 0;
                        self.refresh(false)
                    }
                    KeyCode::Backspace => {
                        self.search.pop();
                    }
                    KeyCode::Char(c) => self.search.push(c),
                    _ => {}
                }
                return;
            }
            Mode::Compose => {
                if key.code == KeyCode::Esc {
                    self.mode = Mode::Browse;
                    return;
                }
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Enter | KeyCode::Char('s'))
                {
                    self.prepare_send();
                    return;
                }
                self.editor.key(key);
                return;
            }
            Mode::Attach => {
                match key.code {
                    KeyCode::Esc => {
                        self.editor = Editor::default();
                        self.mode = Mode::Compose
                    }
                    KeyCode::Enter => {
                        let p = std::path::PathBuf::from(self.editor.text.trim());
                        self.editor = Editor::default();
                        self.upload(p)
                    }
                    _ => self.editor.key(key),
                }
                return;
            }
            Mode::React => {
                match key.code {
                    KeyCode::Esc => self.mode = Mode::Browse,
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.reaction = self.reaction.saturating_sub(1)
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        self.reaction = (self.reaction + 1).min(REACTIONS.len() - 1)
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.reaction = self.reaction.saturating_sub(6)
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.reaction = (self.reaction + 6).min(REACTIONS.len() - 1)
                    }
                    KeyCode::Enter => {
                        if let (Some(c), Some(m)) = (self.active.clone(), self.selected_message()) {
                            self.operation(
                                "Reaction queued",
                                Api::conversation(&c.id, "reaction"),
                                json!({"messageID":m.id,"reactionKey":REACTIONS[self.reaction]}),
                            );
                            self.mode = Mode::Browse
                        }
                    }
                    _ => {}
                }
                return;
            }
            Mode::Accounts => {
                let n = self.accounts["items"].as_array().map(Vec::len).unwrap_or(0);
                match key.code {
                    KeyCode::Esc => self.mode = Mode::Browse,
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.account_selection =
                            (self.account_selection + 1).min(n.saturating_sub(1))
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.account_selection = self.account_selection.saturating_sub(1)
                    }
                    KeyCode::Enter => {
                        if let Some(id) = self.accounts["items"][self.account_selection]["id"]
                            .as_str()
                            .map(str::to_owned)
                        {
                            if id == "discord"
                                && self.accounts["items"][self.account_selection]["info"]["Discord"]
                                    ["logged_in"]
                                    == true
                            {
                                self.open_servers();
                            } else {
                                self.start_login(id, None);
                            }
                        }
                    }
                    _ => {}
                }
                return;
            }
            Mode::Login => {
                if key.code == KeyCode::Esc {
                    self.mode = Mode::Accounts;
                    return;
                }
                if self.login["type"] == "complete" {
                    self.mode = Mode::Browse;
                    return;
                }
                if self.login["type"] == "error" && key.code == KeyCode::Enter {
                    self.start_login(self.login_id.clone(), None);
                    return;
                }
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Enter | KeyCode::Char('s'))
                {
                    self.submit_login();
                    return;
                }
                let fields = self.fields();
                if key.code == KeyCode::Tab {
                    self.login_field = (self.login_field + 1) % fields.len().max(1);
                    return;
                }
                if let Some(f) = fields.get(self.login_field) {
                    let id = f["id"].as_str().unwrap_or("value").to_owned();
                    let s = self.login_values.entry(id).or_default();
                    match key.code {
                        KeyCode::Char(c) => s.push(c),
                        KeyCode::Backspace => {
                            s.pop();
                        }
                        _ => {}
                    }
                }
                return;
            }
            Mode::Browse => {}
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('3') | KeyCode::Char('s') => self.open_servers(),
            KeyCode::Char(c @ '1'..='7') => {
                self.source = (c as u8 - b'1') as usize;
                self.discord_scope = None;
                self.selection = 0;
                self.refresh(false)
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if self.focus == Focus::Inbox {
                    Focus::Messages
                } else {
                    Focus::Inbox
                }
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.search.clear()
            }
            KeyCode::Char('o') => {
                if self.image_target().is_some() {
                    self.mode = Mode::Image;
                    self.focus = Focus::Messages;
                } else {
                    self.notice = "Select a message with an image; o opens its gallery.".into();
                }
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('u') => {
                self.unread = !self.unread;
                self.selection = 0;
                self.refresh(false)
            }
            KeyCode::Char('a') => self.load_accounts(),
            KeyCode::Char('c') => {
                if self.focus == Focus::Messages
                    && let Some(id) = self.selected_message().and_then(|m| m.sender_contact_id)
                {
                    self.open_contact(json!({"id":id}));
                } else {
                    self.open_contacts();
                }
            }
            KeyCode::Char('C') => self.open_contacts(),
            KeyCode::Char('v') => {
                if self.discord_scope.is_some() {
                    self.server_focus = !self.server_focus;
                } else {
                    self.focus_view = !self.focus_view;
                }
                self.selection = 0;
                self.refresh(false);
            }
            KeyCode::Char('*') | KeyCode::Char('z') | KeyCode::Char('=') => {
                let chat = if self.focus == Focus::Messages {
                    self.active.clone()
                } else {
                    self.selected()
                };
                if let Some(c) = chat {
                    let priority = match key.code {
                        KeyCode::Char('*') if c.importance["rule"] != "important" => "important",
                        KeyCode::Char('z') if c.importance["rule"] != "quiet" => "quiet",
                        _ => "auto",
                    };
                    self.operation(
                        "Local importance rule saved",
                        "/api/preferences/conversation".into(),
                        json!({"conversationID":c.id,"priority":priority}),
                    );
                }
            }

            KeyCode::Char('R') => self.refresh_now(),
            KeyCode::Char('n') => {
                if self.focus == Focus::Inbox {
                    self.refresh(true)
                } else if self.message_cursor.is_some() {
                    self.load_messages(true)
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.focus == Focus::Inbox {
                    self.selection =
                        (self.selection + 1).min(self.visible().len().saturating_sub(1))
                } else {
                    self.message_selection =
                        (self.message_selection + 1).min(self.messages.len().saturating_sub(1))
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.focus == Focus::Inbox {
                    self.selection = self.selection.saturating_sub(1)
                } else {
                    self.message_selection = self.message_selection.saturating_sub(1)
                }
            }
            KeyCode::Char('g') | KeyCode::Home => {
                if self.focus == Focus::Inbox {
                    self.selection = 0
                } else {
                    self.message_selection = 0
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                if self.focus == Focus::Inbox {
                    self.selection = self.visible().len().saturating_sub(1)
                } else {
                    self.message_selection = self.messages.len().saturating_sub(1)
                }
            }
            KeyCode::Enter | KeyCode::Char('l') => {
                if self.focus == Focus::Inbox
                    && let Some(c) = self.selected()
                {
                    if self.demo {
                        self.active = Some(c);
                        self.focus = Focus::Messages
                    } else {
                        self.open(c)
                    }
                }
            }
            KeyCode::Char('h') => self.focus = Focus::Inbox,
            KeyCode::Char('i') => self.begin_compose(false),
            KeyCode::Char('r') => self.begin_compose(true),
            KeyCode::Char('e') => {
                if self.active.as_ref().is_some_and(|c| c.can_react) {
                    self.mode = Mode::React;
                    self.reaction = 0
                } else {
                    self.notice = "This connection does not support emoji reactions.".into()
                }
            }
            KeyCode::Char('E') => {
                if let Some(m) = self.selected_message()
                    && m.is_sender
                    && self.active.as_ref().is_some_and(|c| c.kind == "chat")
                {
                    self.editor.set(m.text);
                    self.edit_message = Some(m.id);
                    self.mode = Mode::Compose
                }
            }
            KeyCode::Char('A') => {
                if let Some(c) = self.active.clone() {
                    self.operation(
                        "Archived",
                        Api::conversation(&c.id, "archive"),
                        json!({"archived":true}),
                    )
                }
            }
            KeyCode::Char('m') => {
                self.set_read_state(true);
            }
            KeyCode::Char('f') => {
                if self.active.as_ref().is_some_and(|c| c.kind == "chat") {
                    self.mode = Mode::Attach;
                    self.editor = Editor::default()
                } else {
                    self.notice = "File sending is available in bridged chat conversations.".into()
                }
            }
            KeyCode::Esc => {
                self.reply = None;
                self.focus = Focus::Inbox
            }
            _ => {}
        }
    }
    pub fn upload(&mut self, path: std::path::PathBuf) {
        self.mode = Mode::Compose;
        let Some(chat) = self.active.as_ref().map(|c| c.id.clone()) else {
            return;
        };
        if let Some(api) = self.api.clone() {
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let r = async {
                    let bytes = tokio::fs::read(&path).await?;
                    if bytes.len() > 20 * 1024 * 1024 {
                        anyhow::bail!("Attachments must be 20 MB or smaller")
                    }
                    let content = base64_encode(&bytes);
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("attachment");
                    api.post(
                        "/api/v1/assets/upload/base64",
                        json!({"content":content,"fileName":name,"mimeType": image::guess_format(&bytes).map(|f| f.to_mime_type()).unwrap_or("application/octet-stream")}),
                    )
                    .await
                }
                .await;
                let _ = tx.send(Response::Uploaded(chat, result(r)));
            });
        }
    }
}
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        s.push(CHARS[((n >> 18) & 63) as usize] as char);
        s.push(CHARS[((n >> 12) & 63) as usize] as char);
        s.push(if c.len() > 1 {
            CHARS[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            CHARS[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_send_and_upload_results_cannot_change_another_conversations_composer() {
        let mut a = App::new(None, true);
        a.active = Some(Conversation {
            id: "second".into(),
            ..Default::default()
        });
        a.editor.set("Keep this unsent text".into());
        a.send_busy = true;
        a.tx.send(Response::Sent("first".into(), Ok(json!({}))))
            .unwrap();
        a.tx.send(Response::Uploaded(
            "first".into(),
            Ok(json!({"uploadID":"first-upload"})),
        ))
        .unwrap();
        a.poll();
        assert!(!a.send_busy);
        assert_eq!(a.editor.text, "Keep this unsent text");
        assert!(a.pending_attachment.is_none());
    }
    #[test]
    fn send_failure_keeps_text_and_allows_retry() {
        let mut a = App::new(None, true);
        let id = a.active.as_ref().unwrap().id.clone();
        a.editor.set("Keep me".into());
        a.send_busy = true;
        a.tx.send(Response::Sent(id, Err("offline".into())))
            .unwrap();
        a.poll();
        assert_eq!(a.editor.text, "Keep me");
        assert!(!a.send_busy);
        assert!(a.notice.contains("offline"));
    }
    #[test]
    fn switching_conversations_retains_reply_and_attachment_and_does_not_resurrect_erased_text() {
        let mut a = App::new(None, true);
        let first = a.active.clone().unwrap();
        let second = Conversation {
            id: "another".into(),
            ..Default::default()
        };
        a.editor.set("Unsent".into());
        a.reply = Some(Message {
            id: "parent".into(),
            ..Default::default()
        });
        a.pending_attachment = Some(json!({"uploadID":"file"}));
        a.open(second.clone());
        a.open(first.clone());
        assert_eq!(a.editor.text, "Unsent");
        assert_eq!(a.reply.as_ref().unwrap().id, "parent");
        assert_eq!(a.pending_attachment.as_ref().unwrap()["uploadID"], "file");
        a.editor.set(String::new());
        a.reply = None;
        a.pending_attachment = None;
        a.open(second);
        a.open(first);
        assert!(a.editor.text.is_empty());
    }
    #[test]
    fn revisiting_a_thread_restores_messages_cursor_and_reading_position_immediately() {
        let mut a = App::new(None, true);
        let first = a.active.clone().unwrap();
        a.message_cursor = Some("older-page".into());
        a.message_selection = 1;
        let expected = a.messages[1].id.clone();
        a.open(Conversation {
            id: "other".into(),
            ..Default::default()
        });
        assert!(a.messages.is_empty());
        a.open(first);
        assert_eq!(a.messages[1].id, expected);
        assert_eq!(a.message_selection, 1);
        assert_eq!(a.message_cursor.as_deref(), Some("older-page"));
    }
    #[test]
    fn a_late_response_from_an_earlier_visit_cannot_replace_newer_messages() {
        let mut a = App::new(None, true);
        let id = a.active.as_ref().unwrap().id.clone();
        let expected = a.messages.last().unwrap().id.clone();
        a.message_request_seq = 2;
        a.tx.send(Response::Messages(
            id,
            1,
            Ok(json!({"items":[{"id":"stale"}]})),
            false,
            false,
        ))
        .unwrap();
        a.poll();
        assert_eq!(a.messages.last().unwrap().id, expected);
    }
    #[test]
    fn editor_preserves_graphemes() {
        let mut e = Editor::default();
        e.insert("hello 👨‍👩‍👧‍👦");
        e.key(KeyEvent::from(KeyCode::Backspace));
        assert_eq!(e.text, "hello ");
        e.key(KeyEvent::from(KeyCode::Left));
        e.insert("é");
        assert_eq!(e.text, "helloé ");
    }
    #[test]
    fn email_send_needs_explicit_confirmation() {
        let mut a = App::new(None, true);
        a.active = Some(Conversation {
            id: "mail:home:x".into(),
            kind: "email".into(),
            can_send: true,
            ..Default::default()
        });
        a.messages = vec![Message {
            id: "x".into(),
            from: "student@example.org".into(),
            subject: "Session".into(),
            message_id_header: "<x@example.org>".into(),
            ..Default::default()
        }];
        a.editor.set("Tuesday works".into());
        a.prepare_send();
        assert_eq!(a.mode, Mode::Confirm);
        assert_eq!(
            a.confirm_body.as_ref().unwrap()["to"],
            "student@example.org"
        );
    }
    #[test]
    fn image_gallery_preserves_message_and_draft_and_selects_original_attachment_index() {
        let mut app = App::new(None, true);
        app.focus = Focus::Messages;
        app.editor.set("unsent draft".into());
        app.messages[app.message_selection].attachments = vec![
            json!({"type":"file"}),
            json!({"type":"img","fileName":"one.jpg"}),
            json!({"mimeType":"image/png","fileName":"two.png"}),
        ];
        let selected = app.message_selection;
        app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Image);
        assert_eq!(app.image_target().unwrap().index, 1);
        app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.image_target().unwrap().index, 2);
        app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.image_target().unwrap().index, 2);
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.message_selection, selected);
        assert_eq!(app.editor.text, "unsent draft");
        assert!(app.confirm_body.is_none());
    }
    #[test]
    fn opening_a_cached_thread_marks_read_before_a_refresh_and_survives_switching() {
        let mut app = App::new(None, true);
        let (queue, mut reads) = unbounded_channel();
        app.read_tx = Some(queue);
        let chat = app.active.clone().unwrap();
        app.open(chat.clone());
        assert_eq!(reads.try_recv().unwrap(), (chat.id.clone(), true));
        assert!(app.pending_read.is_none());
        let mut next = chat.clone();
        next.id = "matrix:other".into();
        app.open(next);
        assert!(reads.try_recv().is_err());
        app.open(chat.clone());
        assert_eq!(reads.try_recv().unwrap(), (chat.id, true));
        app.set_read_state(false);
        assert!(!reads.try_recv().unwrap().1);
    }
    #[test]
    fn opening_marks_read_once_and_manual_unread_survives_refresh() {
        let mut app = App::new(None, true);
        let (queue, mut reads) = unbounded_channel();
        app.read_tx = Some(queue);
        let chat = app.active.clone().unwrap();
        app.messages.clear();
        app.open(chat.clone());
        let page = json!({"items":[]});
        let _ = app.tx.send(Response::Messages(
            chat.id.clone(),
            app.message_request_seq,
            Ok(page.clone()),
            false,
            false,
        ));
        app.poll();
        assert_eq!(reads.try_recv().unwrap(), (chat.id.clone(), true));
        app.set_read_state(false);
        assert_eq!(reads.try_recv().unwrap(), (chat.id.clone(), false));
        let _ = app.tx.send(Response::Messages(
            chat.id.clone(),
            app.message_request_seq,
            Ok(page.clone()),
            false,
            true,
        ));
        app.poll();
        assert!(reads.try_recv().is_err());
        app.open(chat.clone());
        app.set_read_state(false);
        assert_eq!(reads.try_recv().unwrap(), (chat.id.clone(), false));
        let _ = app.tx.send(Response::Messages(
            chat.id.clone(),
            app.message_request_seq,
            Ok(page.clone()),
            false,
            false,
        ));
        app.poll();
        assert!(reads.try_recv().is_err());
        let mut archive = chat;
        archive.kind = "archive".into();
        app.open(archive.clone());
        let _ = app.tx.send(Response::Messages(
            archive.id.clone(),
            app.message_request_seq,
            Ok(page),
            false,
            false,
        ));
        app.poll();
        assert!(reads.try_recv().is_err());
    }
    #[test]
    fn slack_thread_navigation_preserves_separate_drafts_and_parent_identity() {
        let mut app = App::new(None, true);
        let mut chat = app.active.clone().unwrap();
        chat.id = "matrix:!room:chord.local".into();
        chat.remote_id = "!room:chord.local".into();
        chat.kind = "chat".into();
        chat.source = "slack".into();
        app.open(chat.clone());
        app.editor.set("channel draft".into());
        let reply = Message {
            id: "$reply".into(),
            thread_root_id: Some("$root".into()),
            ..Default::default()
        };
        app.open_thread(&reply);
        assert_eq!(
            app.active.as_ref().unwrap().thread_root_id.as_deref(),
            Some("$root")
        );
        assert!(app.pending_read.is_none());
        app.editor.set("thread draft".into());
        app.back_to_channel();
        assert_eq!(app.active.as_ref().unwrap().id, chat.id);
        assert_eq!(app.editor.text, "channel draft");
        app.open_thread(&reply);
        assert_eq!(app.editor.text, "thread draft");
    }
    #[test]
    fn text_base64() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }
    #[test]
    fn contact_link_requires_review_and_cancel_preserves_the_source() {
        let mut app = App::new(None, true);
        let source = json!({"id":"person_a", "name":"Alex in Slack"});
        let target = json!({"id":"person_b", "name":"Alex in Discord"});
        app.contacts = vec![source.clone(), target.clone()];
        app.details = source.clone();
        app.mode = Mode::Details;
        app.key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::ContactLink);
        assert_eq!(app.visible_contacts().len(), 1);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::ConfirmContactLink);
        assert_eq!(app.link_source["id"], source["id"]);
        assert_eq!(app.details["id"], target["id"]);
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Details);
        assert_eq!(app.details["id"], source["id"]);
    }
    #[test]
    fn quiet_servers_are_hidden_until_searched_or_explicitly_revealed() {
        let mut app = App::new(None, true);
        app.servers = vec![
            json!({"id":"1","name":"SDC Buggy","preferred":true}),
            json!({"id":"2","name":"Other server","preferred":false}),
        ];
        assert_eq!(app.visible_servers().len(), 3);
        app.server_query = "other".into();
        assert_eq!(app.visible_servers()[0]["id"], "2");
        app.server_query.clear();
        app.show_other_servers = true;
        assert_eq!(app.visible_servers().len(), 4);
    }
    #[test]
    fn selecting_a_discord_server_shows_channels_without_changing_global_focus() {
        let mut app = App::new(None, true);
        app.servers = vec![json!({"id":"123","name":"SDC Buggy","channelCount":23})];
        app.unread = true;
        app.key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Servers);
        app.paste("sdc");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.source, 2);
        assert_eq!(app.discord_scope.as_deref(), Some("123"));
        assert!(!app.focus_enabled());
        assert!(!app.unread);
        assert!(app.focus_view);
        app.key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert!(app.discord_scope.is_none());
        assert!(app.focus_enabled());
    }
    #[test]
    fn background_messages_keep_the_reading_position_and_loaded_history() {
        let mut app = App::new(None, true);
        let room = app.active.as_ref().unwrap().id.clone();
        app.messages = vec![
            Message {
                id: "old".into(),
                timestamp: "2026-10-29T00:00:00Z".into(),
                ..Default::default()
            },
            Message {
                id: "recent".into(),
                timestamp: "2026-10-30T00:00:00Z".into(),
                ..Default::default()
            },
        ];
        app.message_selection = 0;
        app.tx.send(Response::Messages(room,app.message_request_seq,Ok(json!({"items":[{"id":"recent","text":"Updated","timestamp":"2026-10-30T00:00:00Z"},{"id":"new","text":"Incoming","timestamp":"2026-10-30T01:00:00Z"}]})),false,true)).unwrap();
        app.poll();
        assert_eq!(app.messages.len(), 3);
        assert_eq!(app.selected_message().unwrap().id, "old");
        assert_eq!(app.messages[1].text, "Updated");
    }
}
