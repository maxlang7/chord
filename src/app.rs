use crate::{
    api::Api,
    model::{Conversation, Inbox, Message, ReactionCount, SOURCES, clean},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    time::Instant,
};
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
    mentions: Vec<DraftMention>,
}

#[derive(Clone)]
struct DraftMention {
    range: std::ops::Range<usize>,
    user_id: String,
}

impl Editor {
    pub fn set(&mut self, s: String) {
        self.cursor = s.len();
        self.text = s;
        self.mentions.clear();
    }

    fn move_mentions(&mut self, range: std::ops::Range<usize>, replacement_len: usize) {
        let shift = replacement_len as isize - range.len() as isize;
        self.mentions.retain_mut(|mention| {
            if mention.range.end <= range.start {
                true
            } else if mention.range.start >= range.end {
                mention.range.start = mention.range.start.saturating_add_signed(shift);
                mention.range.end = mention.range.end.saturating_add_signed(shift);
                true
            } else {
                false
            }
        });
    }

    pub fn reconcile_mentions(&mut self, before: &str) {
        if self.mentions.is_empty() || before == self.text {
            return;
        }
        let start = before
            .chars()
            .zip(self.text.chars())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        let suffix = before[start..]
            .chars()
            .rev()
            .zip(self.text[start..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        self.move_mentions(
            start..before.len() - suffix,
            self.text.len() - suffix - start,
        );
    }

    pub fn replace_completion(
        &mut self,
        range: std::ops::Range<usize>,
        replacement: &str,
        mention_user: Option<String>,
    ) {
        self.move_mentions(range.clone(), replacement.len());
        if let Some(user_id) = mention_user {
            self.mentions.push(DraftMention {
                range: range.start..range.start + replacement.trim_end().len(),
                user_id,
            });
            self.mentions.sort_by_key(|mention| mention.range.start);
        }
        self.text.replace_range(range, replacement);
    }

    fn mention_payload(&self) -> Option<(String, BTreeSet<&str>)> {
        let mut users = BTreeSet::new();
        let mut html = String::new();
        let mut cursor = 0;
        for mention in &self.mentions {
            let range = &mention.range;
            let Some(label) = self
                .text
                .get(range.clone())
                .filter(|label| label.starts_with('@'))
            else {
                continue;
            };
            if range.start < cursor
                || self.text[range.end..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            append_html(&mut html, &self.text[cursor..range.start]);
            html.push_str("<a href=\"https://matrix.to/#/");
            html.push_str(&urlencoding::encode(&mention.user_id));
            html.push_str("\">");
            append_html(&mut html, label);
            html.push_str("</a>");
            users.insert(mention.user_id.as_str());
            cursor = range.end;
        }
        append_html(&mut html, &self.text[cursor..]);
        (!users.is_empty()).then_some((html, users))
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

fn append_html(html: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            '\n' => html.push_str("<br>"),
            _ => html.push(c),
        }
    }
}

pub enum Response {
    Image(String, Result<crate::images::Decoded, String>),
    Inbox(u64, Result<Value, String>, bool),
    Messages(String, u64, Result<Value, String>, bool, bool),
    InlineThread(String, Result<Value, String>),
    Operation(String, Result<Value, String>),
    ReadState(String, u64, bool, Result<Value, String>),
    Sent(String, String, Result<Value, String>),
    Uploaded(String, Result<Value, String>),
    Accounts(Result<Value, String>),
    Login(String, Result<Value, String>),
    Reaction(u64, Result<Value, String>),
    #[cfg(target_os = "macos")]
    WebLogin(String, Result<HashMap<String, String>, String>),
}
fn result(r: anyhow::Result<Value>) -> Result<Value, String> {
    r.map_err(|e| format!("{e:#}"))
}

pub fn reaction_is_users(reaction: &Value, key: &str, self_id: &str) -> bool {
    if reaction["reactionKey"].as_str() != Some(key) {
        return false;
    }
    reaction["participantName"].as_str() == Some("You")
        || reaction["local"] == true
        || (!self_id.is_empty() && reaction["participantID"].as_str() == Some(self_id))
}

pub fn user_has_reaction(reactions: &[Value], key: &str, self_id: &str) -> bool {
    reactions
        .iter()
        .any(|reaction| reaction_is_users(reaction, key, self_id))
}

fn apply_user_reaction(reactions: &mut Vec<Value>, key: &str, self_id: &str, add: bool) {
    let present = user_has_reaction(reactions, key, self_id);
    if add && !present {
        reactions.push(json!({
            "reactionKey": key,
            "participantID": if self_id.is_empty() { "you" } else { self_id },
            "participantName": "You",
            "local": true,
        }));
    } else if !add && present {
        reactions.retain(|reaction| !reaction_is_users(reaction, key, self_id));
    }
}

fn now_stamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn keeps_failure(notice: &str) -> bool {
    notice.starts_with("Message not sent") || notice.starts_with("Reaction didn't")
}

struct ReadOverride {
    revision: u64,
    read: bool,
    previous_unread: bool,
    timestamp: String,
    acknowledged_after: Option<u64>,
}

#[derive(Clone)]
struct PendingReaction {
    chat_id: String,
    message_id: String,
    key: String,
    desired: bool,
    /// Revision and the add/remove state that request carried.
    sent: Option<(u64, bool)>,
    accepted: bool,
}

#[derive(Clone)]
struct Outgoing {
    chat_id: String,
    token: String,
    edit_id: Option<String>,
    previous_text: Option<String>,
    text: String,
    timestamp: String,
    body: Value,
    accepted: bool,
    error: Option<String>,
    server_id: Option<String>,
}

#[derive(Clone, Default)]
struct Draft {
    text: String,
    mentions: Vec<DraftMention>,
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
    reaction_seq: u64,
    outgoing_seq: u64,
    pending_reactions: Vec<PendingReaction>,
    outgoing: Vec<Outgoing>,
    pub native_ui: bool,
    pub images: crate::images::Images,
    pub api: Option<Api>,
    pub demo: bool,
    pub source: usize,
    pub hidden_accounts: BTreeSet<String>,
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
    inbox_paged: bool,
    inbox_keep_tail: bool,
    pub scroll: u16,
    pub quit: bool,
    pub account_selection: usize,
    pub accounts: Value,
    pub login_id: String,
    pub login: Value,
    pub login_field: usize,
    pub login_values: HashMap<String, String>,
    pub login_busy: bool,
    // Per-tab (SOURCES key) conversation IDs the user dragged into a pinned group, in order.
    pub pins: HashMap<String, Vec<String>>,
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
    opening_unread: Option<(Option<i64>, usize)>,
    pub first_unread: Option<String>,
    read_tx: Option<UnboundedSender<(String, u64, bool)>>,
    read_revision: u64,
    read_overrides: HashMap<String, ReadOverride>,
    pub archive_status: Value,
    pub last_connection_refresh: Instant,
    pub show_unimportant: bool,
    pub unimportant_discord_servers: BTreeSet<String>,
    pub discord_scope: Option<String>,
    pub server_label: String,
    pub servers: Vec<Value>,
    pub server_query: String,
    pub show_other_servers: bool,
    pub server_selection: usize,
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
            let (queue, mut reads) = unbounded_channel::<(String, u64, bool)>();
            let responses = tx.clone();
            tokio::spawn(async move {
                while let Some((id, revision, read)) = reads.recv().await {
                    let route = Api::conversation(&id, if read { "read" } else { "unread" });
                    let response = result(api.post(&route, json!({})).await);
                    let _ = responses.send(Response::ReadState(id, revision, read, response));
                }
            });
            queue
        });
        let mut app = Self {
            send_busy: false,
            reaction_seq: 0,
            outgoing_seq: 0,
            pending_reactions: Vec::new(),
            outgoing: Vec::new(),
            native_ui: false,
            images: crate::images::Images::default(),
            api,
            demo,
            source: 0,
            hidden_accounts: BTreeSet::new(),
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
            inbox_paged: false,
            inbox_keep_tail: false,
            scroll: 0,
            quit: false,
            account_selection: 0,
            accounts: Value::Null,
            login_id: String::new(),
            login: Value::Null,
            login_field: 0,
            login_values: HashMap::new(),
            login_busy: false,
            pins: HashMap::new(),
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
            opening_unread: None,
            first_unread: None,
            read_revision: 0,
            read_overrides: HashMap::new(),
            read_tx,
            archive_status: Value::Null,
            last_connection_refresh: Instant::now(),
            show_unimportant: false,
            unimportant_discord_servers: BTreeSet::new(),
            discord_scope: None,
            server_label: String::new(),
            servers: vec![],
            server_query: String::new(),
            show_other_servers: false,
            server_selection: 0,
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
    pub fn account_visible(&self, conversation: &Conversation) -> bool {
        !self.hidden_accounts.contains(&conversation.source)
            && !self.hidden_accounts.contains(&conversation.account_key)
    }

    pub fn visible(&self) -> Vec<&Conversation> {
        let mut chats: Vec<&Conversation> = self
            .conversations
            .iter()
            .filter(|c| {
                self.account_visible(c)
                    && (self.source == 0 || c.source == SOURCES[self.source].0)
                    && (!self.unread || c.unread)
                    && (self.show_unimportant
                        || (c.local_priority != "unimportant"
                            && !(c.source == "discord"
                                && c.server_id.as_ref().is_some_and(|id| {
                                    self.unimportant_discord_servers.contains(id)
                                }))))
            })
            .collect();
        let pins = self.tab_pins();
        // Stable, so unpinned chats keep the inbox's most-recent-first order.
        chats.sort_by_key(|c| pins.iter().position(|id| *id == c.id).unwrap_or(usize::MAX));
        chats
    }
    // An app tab shows its own pins first, then anything pinned in All accounts.
    fn tab_pins(&self) -> Vec<String> {
        let key = SOURCES[self.source].0;
        let mut pins = self.pins.get(key).cloned().unwrap_or_default();
        if key != SOURCES[0].0 {
            for id in self.pins.get(SOURCES[0].0).into_iter().flatten() {
                if !pins.contains(id) {
                    pins.push(id.clone());
                }
            }
        }
        pins
    }
    pub fn is_pinned(&self, id: &str) -> bool {
        self.tab_pins().iter().any(|p| p == id)
    }
    pub fn pinned_count(&self) -> usize {
        self.visible()
            .iter()
            .filter(|c| self.is_pinned(&c.id))
            .count()
    }
    // Reorder within the stored tab, preserving hidden rows and inherited pins.
    pub fn move_pin(&mut self, id: &str, position: Option<usize>) {
        let global = SOURCES[0].0;
        let tab = SOURCES[self.source].0;
        let owns = |key: &str| {
            self.pins
                .get(key)
                .is_some_and(|pins| pins.iter().any(|pin| pin == id))
        };
        let owner = if !owns(tab) && owns(global) {
            global
        } else {
            tab
        };
        let shown = self
            .visible()
            .into_iter()
            .filter(|c| c.id != id && self.is_pinned(&c.id))
            .map(|c| c.id.clone())
            .collect::<Vec<_>>();
        let order = self.pins.entry(owner.into()).or_default();
        order.retain(|pin| pin != id);
        if let Some(position) = position {
            let index = shown
                .iter()
                .skip(position)
                .find_map(|anchor| order.iter().position(|pin| pin == anchor))
                .unwrap_or(order.len());
            order.insert(index, id.into());
        }
        let active = self.active.as_ref().map(|c| c.id.clone());
        if let Some(active) = active
            && let Some(index) = self.visible().iter().position(|c| c.id == active)
        {
            self.selection = index;
        }
    }
    pub fn selected(&self) -> Option<Conversation> {
        self.visible().get(self.selection).map(|c| (*c).clone())
    }
    pub fn selected_message(&self) -> Option<Message> {
        self.messages.get(self.message_selection).cloned()
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
        self.refresh_discord_servers();
    }
    pub fn refresh_discord_servers(&self) {
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
        self.refresh_inbox(append, false, false);
    }
    /// Apply a right-click rule immediately. A mute stops counting as unread
    /// until the fresh inbox agrees. Passing `mute_minutes` of `Some(None)` unmutes.
    pub fn apply_channel_rule(
        &mut self,
        id: &str,
        priority: Option<&str>,
        mute_minutes: Option<Option<i64>>,
    ) {
        let now = chrono::Utc::now().timestamp_millis();
        let apply = |chat: &mut Conversation| {
            if chat.id != id {
                return;
            }
            if let Some(priority) = priority {
                chat.local_priority = priority.to_owned();
            }
            if let Some(minutes) = mute_minutes {
                chat.mute_until = minutes.map(|minutes| now + minutes * 60_000);
            }
            let muted = chat.mute_until.is_some_and(|until| until > now);
            if muted {
                chat.unread = false;
                chat.unread_count = 0;
            }
        };
        for chat in &mut self.conversations {
            apply(chat);
        }
        if let Some(chat) = &mut self.active {
            apply(chat);
        }
        self.read_overrides.remove(id);
    }
    pub fn set_unimportant(&mut self, id: &str, unimportant: bool) {
        let priority = if unimportant { "unimportant" } else { "auto" };
        if self.demo {
            self.apply_channel_rule(id, Some(priority), None);
        } else {
            self.operation(
                "Inbox visibility updated",
                "/api/preferences/conversation".into(),
                json!({"conversationID":id,"priority":priority}),
            );
        }
    }

    pub fn has_more_inbox(&self) -> bool {
        self.inbox_cursors
            .as_object()
            .is_some_and(|cursors| !cursors.is_empty())
    }

    pub fn refresh_now(&mut self) {
        self.refresh_inbox(false, true, false);
        self.request_messages(false, !self.messages.is_empty(), true);
        self.load_connections();
    }
    fn refresh_inbox(&mut self, append: bool, fresh: bool, keep_tail: bool) {
        if self.demo || (append && (self.loading || !self.has_more_inbox())) {
            return;
        }
        self.inbox_keep_tail = keep_tail;
        if append {
            self.inbox_paged = true;
        } else if !keep_tail {
            self.inbox_paged = false;
            self.inbox_cursors = Value::Null;
        }
        self.request_seq += 1;
        let seq = self.request_seq;
        self.loading = true;
        self.last_refresh = Instant::now();
        if let Some(api) = self.api.clone() {
            let api = if fresh { api.fresh() } else { api };
            let source = SOURCES[self.source].0.to_string();
            let query = self.search.clone();
            let view = if self.show_unimportant {
                "all"
            } else {
                "inbox"
            };
            let unread = self.unread;
            let hidden_servers: Vec<_> = if self.show_unimportant {
                Vec::new()
            } else {
                self.unimportant_discord_servers.iter().cloned().collect()
            };
            let pins = self.tab_pins();
            let server = self.discord_scope.clone();
            let cursor = append.then(|| self.inbox_cursors.to_string());
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
                        &hidden_servers,
                        &pins,
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
                            &hidden_servers,
                            &pins,
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
        self.first_unread = None;
        self.opening_unread = c
            .unread
            .then_some((c.local_read_at.filter(|t| *t > 0), c.unread_count.max(1)));
        self.remember_messages();
        let cached = self.message_cache.get(&c.id).cloned();
        if let Some(old) = &self.active {
            let draft = Draft {
                text: self.editor.text.clone(),
                mentions: self.editor.mentions.clone(),
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
        self.editor.mentions = draft.mentions;
        self.reply = draft.reply;
        self.edit_message = draft.edit_message;
        self.pending_attachment = draft.attachment;
        self.pending_read =
            (c.kind != "archive" && c.thread_root_id.is_none()).then(|| c.id.clone());
        self.active = Some(c);
        if self.pending_read.is_some() {
            let id = self.pending_read.clone().unwrap();
            self.optimistic_read(&id, true);
        }
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
        self.reconcile_outgoing();
        self.overlay_reactions();
        self.scroll = 0;
        if !self.messages.is_empty() && self.pending_read.is_some() {
            self.set_read_state(true);
        }
        self.load_messages(false);
    }
    fn locate_first_unread(&mut self) {
        let Some((after, count)) = self.opening_unread else {
            return;
        };
        if self.messages.is_empty() {
            return;
        }
        let need_older = if let Some(after) = after {
            self.messages
                .first()
                .and_then(|m| chrono::DateTime::parse_from_rfc3339(&m.timestamp).ok())
                .is_some_and(|time| time.timestamp_millis() > after)
        } else {
            self.messages.len() < count
        };
        if need_older && self.message_cursor.is_some() {
            self.load_messages(true);
            return;
        }
        let index = after
            .and_then(|after| {
                self.messages.iter().position(|m| {
                    !m.is_sender
                        && chrono::DateTime::parse_from_rfc3339(&m.timestamp)
                            .is_ok_and(|time| time.timestamp_millis() > after)
                })
            })
            .unwrap_or_else(|| self.messages.len().saturating_sub(count));
        self.message_selection = index;
        self.first_unread = self.messages.get(index).map(|m| m.id.clone());
        self.opening_unread = None;
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
    pub fn load_inline_thread(&self, root: &str) {
        let Some(chat) = self.active.clone() else {
            return;
        };
        let Some(api) = self.api.clone() else { return };
        let id = format!(
            "matrix-thread:{}:{}",
            urlencoding::encode(&chat.remote_id),
            urlencoding::encode(root)
        );
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let response = api.fresh().messages(&id, None).await;
            let _ = tx.send(Response::InlineThread(chat.id, result(response)));
        });
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
    fn apply_read_locally(&mut self, id: &str, unread: bool) {
        for c in &mut self.conversations {
            if c.id == id {
                c.unread = unread;
            }
        }
        if let Some(c) = &mut self.active
            && c.id == id
        {
            c.unread = unread;
        }
    }
    fn optimistic_read(&mut self, id: &str, read: bool) -> u64 {
        let previous_unread = self
            .read_overrides
            .get(id)
            .filter(|old| old.read == read)
            .map(|old| old.previous_unread)
            .unwrap_or_else(|| {
                self.conversations
                    .iter()
                    .find(|c| c.id == id)
                    .or_else(|| self.active.as_ref().filter(|c| c.id == id))
                    .is_some_and(|c| c.unread)
            });
        let timestamp = self
            .conversations
            .iter()
            .find(|c| c.id == id)
            .or_else(|| self.active.as_ref().filter(|c| c.id == id))
            .map(|c| c.timestamp.clone())
            .unwrap_or_default();
        self.read_revision += 1;
        self.read_overrides.insert(
            id.into(),
            ReadOverride {
                revision: self.read_revision,
                read,
                previous_unread,
                timestamp,
                acknowledged_after: None,
            },
        );
        self.apply_read_locally(id, !read);
        self.read_revision
    }
    pub fn set_read_state(&mut self, read: bool) {
        self.pending_read = None;
        if let Some(c) = &self.active
            && c.kind != "archive"
        {
            let id = c.id.clone();
            let revision = self.optimistic_read(&id, read);
            if let Some(queue) = &self.read_tx {
                let _ = queue.send((id, revision, read));
            }
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
            self.reply = self
                .selected_message()
                .filter(|message| !message.is_local_outgoing());
            if self.reply.is_none() {
                self.notice = "Select a synced message to reply.".into();
                return;
            }
        }
        self.mode = Mode::Compose;
    }
    pub fn prepare_send(&mut self) -> Option<String> {
        if self.editor.text.trim().is_empty() && self.pending_attachment.is_none() {
            return None;
        }
        let c = self.active.clone()?;
        if self.reply.is_some() && self.pending_attachment.is_some() && c.kind == "chat" {
            self.notice = "Send the text reply and attachment separately. This bridge cannot attach a native reply to media yet.".into();
            return None;
        }
        let mut body = json!({"text":self.editor.text});
        if c.kind == "email" {
            let m = self
                .reply
                .clone()
                .or_else(|| self.messages.last().cloned())?;
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
            if let Some((html, users)) = self.editor.mention_payload() {
                if self.pending_attachment.is_some() {
                    self.notice = "Send the tagged message and attachment separately.".into();
                    return None;
                }
                body["html"] = json!(html);
                body["mentions"] = json!({"userIds": users});
            }
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
        if self.native_ui { self.confirm() } else { None }
    }
    pub fn confirm(&mut self) -> Option<String> {
        let (Some(c), Some(mut b)) = (self.active.clone(), self.confirm_body.take()) else {
            return None;
        };
        if let Some(id) = self.edit_message.clone() {
            b["messageID"] = json!(id)
        }
        let shown = self.stage_outgoing(&c, b);
        self.drafts.remove(&c.id);
        self.editor = Editor::default();
        self.reply = None;
        self.edit_message = None;
        self.pending_attachment = None;
        self.mode = Mode::Browse;
        self.send_busy = false;
        if self.notice.starts_with("Message not sent") || self.notice.starts_with("Submitting") {
            self.notice.clear();
        }
        self.remember_messages();
        Some(shown)
    }
    fn self_id(&self) -> String {
        self.status["userID"].as_str().unwrap_or("").to_owned()
    }
    fn stage_outgoing(&mut self, chat: &Conversation, body: Value) -> String {
        self.outgoing_seq += 1;
        let token = format!("local-{}", self.outgoing_seq);
        let edit_id = body["messageID"].as_str().map(str::to_owned);
        let previous_text = edit_id.as_ref().and_then(|id| {
            self.messages
                .iter()
                .find(|message| &message.id == id)
                .map(|message| message.text.clone())
        });
        let shown = edit_id.clone().unwrap_or_else(|| token.clone());
        let item = Outgoing {
            chat_id: chat.id.clone(),
            token,
            edit_id,
            previous_text,
            text: body["text"].as_str().unwrap_or("").to_owned(),
            timestamp: now_stamp(),
            body,
            accepted: false,
            error: None,
            server_id: None,
        };
        Self::submit_outgoing(self.api.as_ref(), &self.tx, &item);
        self.outgoing.push(item);
        self.reconcile_outgoing();
        if let Some(index) = self.messages.iter().position(|message| message.id == shown) {
            self.message_selection = index;
        }
        shown
    }

    fn submit_outgoing(api: Option<&Api>, tx: &UnboundedSender<Response>, item: &Outgoing) {
        let Some(api) = api.cloned() else { return };
        let route = Api::conversation(
            &item.chat_id,
            if item.edit_id.is_some() {
                "edit"
            } else {
                "send"
            },
        );
        let tx = tx.clone();
        let item = item.clone();
        tokio::spawn(async move {
            let r = api.post(&route, item.body).await;
            let _ = tx.send(Response::Sent(item.chat_id, item.token, result(r)));
        });
    }

    pub fn retry_outgoing(&mut self, token: &str) {
        let Some(chat) = &self.active else { return };
        let Some(item) = self
            .outgoing
            .iter_mut()
            .find(|item| item.chat_id == chat.id && item.token == token && item.error.is_some())
        else {
            return;
        };
        item.error = None;
        Self::submit_outgoing(self.api.as_ref(), &self.tx, item);
        self.notice.clear();
        self.reconcile_outgoing();
        self.remember_messages();
    }

    pub fn dismiss_outgoing(&mut self, token: &str) {
        let Some(chat) = &self.active else { return };
        self.outgoing.retain(|item| {
            item.chat_id != chat.id
                || item.token != token
                || (item.error.is_none() && !item.accepted)
        });
        if !self.outgoing.iter().any(|item| item.token == token) {
            self.messages.retain(|message| message.id != token);
            self.message_selection = self
                .message_selection
                .min(self.messages.len().saturating_sub(1));
            self.remember_messages();
        }
    }

    fn fail_outgoing(&mut self, chat_id: &str, token: &str, error: &str) {
        let Some(item) = self
            .outgoing
            .iter_mut()
            .find(|item| item.token == token && item.chat_id == chat_id)
        else {
            return;
        };
        item.error = Some(error.to_owned());
        let active = self.active.as_ref().is_some_and(|chat| chat.id == chat_id);
        let messages = if active {
            Some(&mut self.messages)
        } else {
            self.message_cache
                .get_mut(chat_id)
                .map(|cached| &mut cached.messages)
        };
        if let (Some(messages), Some(edit_id), Some(previous)) =
            (messages, &item.edit_id, &item.previous_text)
            && let Some(message) = messages
                .iter_mut()
                .find(|message| &message.id == edit_id && message.text == item.text)
        {
            message.text = previous.clone();
        }
        if active {
            self.reconcile_outgoing();
            self.remember_messages();
        }
        self.notice = format!("Message not sent. {error}");
    }

    fn reconcile_outgoing(&mut self) {
        let Some(chat) = &self.active else { return };
        self.messages
            .retain(|message| message.send_status.is_none() && message.send_error.is_none());
        self.outgoing.retain(|item| {
            if item.chat_id != chat.id {
                return true;
            }
            if item.error.is_none() {
                if let Some(edit_id) = &item.edit_id {
                    if let Some(message) = self
                        .messages
                        .iter_mut()
                        .find(|message| &message.id == edit_id)
                    {
                        if item.accepted && message.text == item.text {
                            return false;
                        }
                        message.text = item.text.clone();
                    }
                    return true;
                }
                if item
                    .server_id
                    .as_ref()
                    .is_some_and(|id| self.messages.iter().any(|message| &message.id == id))
                {
                    return false;
                }
            }
            let attachments = item.body["attachment"]["fileName"]
                .as_str()
                .map(|name| vec![json!({"fileName":name,"type":"file"})])
                .unwrap_or_default();
            let send_status = item.error.is_none().then(|| {
                if item.accepted {
                    "Sent · awaiting sync"
                } else {
                    "Sending…"
                }
                .into()
            });
            self.messages.push(Message {
                id: item.token.clone(),
                text: item.text.clone(),
                sender_name: "You".into(),
                is_sender: true,
                timestamp: item.timestamp.clone(),
                linked_message_id: item.body["replyToMessageID"].as_str().map(str::to_owned),
                attachments,
                send_error: item.error.clone(),
                send_status,
                ..Default::default()
            });
            true
        });
        self.messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }
    pub fn queue_reaction(&mut self, chat_id: &str, message_id: &str, key: &str, add: bool) {
        if chat_id.is_empty() || message_id.is_empty() || key.is_empty() {
            return;
        }
        self.apply_reaction_local(message_id, key, add);
        if self.demo {
            return;
        }
        if let Some(pending) = self.pending_reactions.iter_mut().find(|pending| {
            pending.chat_id == chat_id && pending.message_id == message_id && pending.key == key
        }) {
            pending.desired = add;
            pending.accepted = false;
        } else {
            self.pending_reactions.push(PendingReaction {
                chat_id: chat_id.to_owned(),
                message_id: message_id.to_owned(),
                key: key.to_owned(),
                desired: add,
                sent: None,
                accepted: false,
            });
        }
        if self.notice.starts_with("Reaction didn't") {
            self.notice.clear();
        }
        self.pump_reactions();
    }
    fn apply_reaction_local(&mut self, message_id: &str, key: &str, add: bool) {
        let self_id = self.self_id();
        if let Some(message) = self
            .messages
            .iter_mut()
            .find(|message| message.id == message_id)
        {
            apply_user_reaction(&mut message.reactions, key, &self_id, add);
            if let Some(counts) = &mut message.reaction_counts {
                if let Some(count) = counts.iter_mut().find(|count| count.key == key) {
                    if count.mine != add {
                        count.mine = add;
                        count.count = if add {
                            count.count.saturating_add(1)
                        } else {
                            count.count.saturating_sub(1)
                        };
                    }
                } else if add {
                    counts.push(ReactionCount {
                        key: key.to_owned(),
                        count: 1,
                        mine: true,
                        shortcode: None,
                    });
                }
            }
        }
    }
    fn pump_reactions(&mut self) {
        let mut jobs = Vec::new();
        for pending in &mut self.pending_reactions {
            if pending.sent.is_some() {
                continue;
            }
            self.reaction_seq += 1;
            let revision = self.reaction_seq;
            pending.sent = Some((revision, pending.desired));
            jobs.push((
                revision,
                pending.chat_id.clone(),
                pending.message_id.clone(),
                pending.key.clone(),
                pending.desired,
            ));
        }
        let Some(api) = self.api.clone() else {
            return;
        };
        let tx = self.tx.clone();
        for (revision, chat, message, key, add) in jobs {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let route = Api::conversation(&chat, "reaction");
                let body = json!({"messageID": message, "reactionKey": key, "remove": !add});
                let response = result(api.post(&route, body).await);
                let _ = tx.send(Response::Reaction(revision, response));
            });
        }
    }
    fn settle_reactions(&mut self) {
        let self_id = self.self_id();
        self.pending_reactions.retain(|pending| {
            if !pending.accepted {
                return true;
            }
            let Some(message) = self
                .messages
                .iter()
                .find(|message| message.id == pending.message_id)
            else {
                return true;
            };
            user_has_reaction(&message.reactions, &pending.key, &self_id) != pending.desired
        });
    }
    fn overlay_reactions(&mut self) {
        let Some(chat_id) = self.active.as_ref().map(|chat| chat.id.clone()) else {
            return;
        };
        let pending: Vec<(String, String, bool)> = self
            .pending_reactions
            .iter()
            .filter(|pending| pending.chat_id == chat_id)
            .map(|pending| {
                (
                    pending.message_id.clone(),
                    pending.key.clone(),
                    pending.desired,
                )
            })
            .collect();
        for (message_id, key, desired) in pending {
            self.apply_reaction_local(&message_id, &key, desired);
        }
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
    #[cfg(target_os = "macos")]
    pub fn web_login(&mut self) {
        if self.login_busy {
            return;
        }
        let id = self.login_id.clone();
        let step = self.login["cookies"].to_string();
        let tx = self.tx.clone();
        self.login_busy = true;
        self.notice = "Finish signing in in the browser window".into();
        tokio::spawn(async move {
            let r = async {
                use tokio::io::AsyncWriteExt;
                let mut child = tokio::process::Command::new(std::env::current_exe()?)
                    .arg("--web-login")
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .kill_on_drop(true)
                    .spawn()?;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(step.as_bytes()).await?;
                }
                let output = child.wait_with_output().await?;
                if !output.status.success() {
                    anyhow::bail!("The sign-in window was closed before signing in finished");
                }
                Ok(serde_json::from_slice(&output.stdout)?)
            }
            .await;
            let _ = tx.send(Response::WebLogin(
                id,
                r.map_err(|e: anyhow::Error| format!("{e:#}")),
            ));
        });
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
                                && let Err(e) = crate::gui::links::open(&file)
                            {
                                self.notice = format!("Could not open the image: {e}");
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
                Response::Sent(id, token, r) => {
                    self.send_busy = false;
                    match r {
                        Ok(value) => {
                            let server_id = value["pendingMessageID"]
                                .as_str()
                                .or_else(|| value["id"].as_str())
                                .map(str::to_owned);
                            let mut is_edit = false;
                            if let Some(item) = self
                                .outgoing
                                .iter_mut()
                                .find(|item| item.chat_id == id && item.token == token)
                            {
                                item.accepted = true;
                                item.server_id = server_id;
                                is_edit = item.edit_id.is_some();
                            }
                            if self.active.as_ref().is_some_and(|chat| chat.id == id) {
                                if !is_edit {
                                    self.reconcile_outgoing();
                                }
                                self.load_messages(false);
                            }
                            self.refresh_inbox(false, false, self.inbox_paged);
                        }
                        Err(error) => self.fail_outgoing(&id, &token, &error),
                    }
                }
                Response::Reaction(revision, response) => {
                    let Some(index) = self
                        .pending_reactions
                        .iter()
                        .position(|pending| pending.sent.is_some_and(|(sent, _)| sent == revision))
                    else {
                        continue;
                    };
                    let sent_add = self.pending_reactions[index].sent.unwrap().1;
                    self.pending_reactions[index].sent = None;
                    match response {
                        Ok(_) => {
                            if self.pending_reactions[index].desired == sent_add {
                                self.pending_reactions[index].accepted = true;
                                let chat_id = self.pending_reactions[index].chat_id.clone();
                                if self.active.as_ref().is_some_and(|chat| chat.id == chat_id) {
                                    self.load_messages(false);
                                }
                            }
                        }
                        Err(error) => {
                            if self.pending_reactions[index].desired == sent_add {
                                let pending = self.pending_reactions.remove(index);
                                self.apply_reaction_local(
                                    &pending.message_id,
                                    &pending.key,
                                    !pending.desired,
                                );
                                self.notice = format!("Reaction didn't go through. {error}");
                            }
                        }
                    }
                    self.pump_reactions();
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
                                Ok(mut data) => {
                                    for c in &mut data.items {
                                        if let Some(state) = self.read_overrides.get(&c.id) {
                                            if !cached
                                                && state
                                                    .acknowledged_after
                                                    .is_some_and(|ack| seq > ack)
                                                && (c.unread != state.read
                                                    || (state.read
                                                        && chrono::DateTime::parse_from_rfc3339(
                                                            &c.timestamp,
                                                        )
                                                        .ok()
                                                        .zip(
                                                            chrono::DateTime::parse_from_rfc3339(
                                                                &state.timestamp,
                                                            )
                                                            .ok(),
                                                        )
                                                        .is_some_and(|(current, prior)| {
                                                            current > prior
                                                        })))
                                            {
                                                self.read_overrides.remove(&c.id);
                                            } else {
                                                c.unread = !state.read;
                                            }
                                        }
                                    }
                                    let selected = self.selected().map(|c| c.id);
                                    if append {
                                        for c in data.items {
                                            if !self.conversations.iter().any(|old| old.id == c.id)
                                            {
                                                self.conversations.push(c)
                                            }
                                        }
                                    } else {
                                        let updated: std::collections::HashSet<_> =
                                            data.items.iter().map(|chat| chat.id.clone()).collect();
                                        if self.inbox_keep_tail {
                                            data.items.extend(
                                                self.conversations
                                                    .drain(..)
                                                    .filter(|chat| !updated.contains(&chat.id)),
                                            );
                                        }
                                        self.conversations = data.items;
                                    }
                                    if append || self.inbox_keep_tail {
                                        self.conversations
                                            .sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                                    }
                                    if !self.inbox_keep_tail {
                                        self.inbox_cursors = data.cursors;
                                    }
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
                                    if !keeps_failure(&self.notice) {
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
                            if !keeps_failure(&self.notice) {
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
                                self.settle_reactions();
                                self.overlay_reactions();
                                self.reconcile_outgoing();
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
                                if v["_cache"]["refreshing"] != true {
                                    self.locate_first_unread();
                                }
                            }
                        }
                        Err(e) => self.notice = e,
                    }
                }
                Response::InlineThread(id, response) => {
                    if self.active.as_ref().map(|c| &c.id) != Some(&id) {
                        continue;
                    }
                    match response {
                        Ok(value) => {
                            match serde_json::from_value::<Vec<Message>>(value["items"].clone()) {
                                Ok(mut replies) => {
                                    let selected = self.selected_message().map(|m| m.id);
                                    replies.extend(self.messages.clone());
                                    let mut seen = std::collections::HashSet::new();
                                    replies.retain(|m| seen.insert(m.id.clone()));
                                    replies.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                                    self.messages = replies;
                                    self.settle_reactions();
                                    self.overlay_reactions();
                                    self.reconcile_outgoing();
                                    if let Some(index) = selected.and_then(|id| {
                                        self.messages.iter().position(|m| m.id == id)
                                    }) {
                                        self.message_selection = index;
                                    }
                                    self.remember_messages();
                                }
                                Err(error) => self.notice = error.to_string(),
                            }
                        }
                        Err(error) => self.notice = error,
                    }
                }
                Response::ReadState(id, revision, read, response) => {
                    if self
                        .read_overrides
                        .get(&id)
                        .is_none_or(|state| state.revision != revision)
                    {
                        continue;
                    }
                    match response {
                        Ok(_) => {
                            self.read_overrides.get_mut(&id).unwrap().acknowledged_after =
                                Some(self.request_seq);
                            self.apply_read_locally(&id, !read);
                            self.refresh(false);
                        }
                        Err(error) => {
                            let state = self.read_overrides.remove(&id).unwrap();
                            self.apply_read_locally(&id, state.previous_unread);
                            self.notice = format!("Could not update read state: {error}");
                        }
                    }
                }
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

                        "Channel updated"
                        | "Inbox visibility updated"
                        | "Channel muted"
                        | "Channel unmuted"
                        | "Local importance rule saved" => {
                            self.notice = label;
                            self.refresh_inbox(false, true, false);
                        }
                        _ => {
                            self.notice = label;
                            self.refresh(false);
                            self.load_messages(false)
                        }
                    },
                    Err(e)
                        if label == "Inbox visibility updated"
                            && e.contains("Priority must be auto") =>
                    {
                        self.notice =
                            "Restart the Chord backend to save Inbox visibility settings.".into();
                    }
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
                #[cfg(target_os = "macos")]
                Response::WebLogin(id, r) => {
                    if self.login_id != id {
                        continue;
                    }
                    self.login_busy = false;
                    match r {
                        Ok(values) => {
                            self.login_values.extend(values);
                            self.notice = "Signed in, connecting…".into();
                            self.submit_login();
                        }
                        Err(e) => self.notice = e,
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
            self.refresh_inbox(false, false, self.inbox_paged);
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
                    KeyCode::Enter | KeyCode::Char('y') => {
                        self.confirm();
                    }
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
                            let key = REACTIONS[self.reaction];
                            let add = !user_has_reaction(&m.reactions, key, &self.self_id());
                            self.queue_reaction(&c.id, &m.id, key, add);
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
                self.show_unimportant = !self.show_unimportant;
                self.selection = 0;
                self.refresh(false);
            }
            KeyCode::Char('*') | KeyCode::Char('=') => {
                let chat = if self.focus == Focus::Messages {
                    self.active.clone()
                } else {
                    self.selected()
                };
                if let Some(chat) = chat {
                    self.set_unimportant(
                        &chat.id,
                        key.code == KeyCode::Char('*') && chat.local_priority != "unimportant",
                    );
                }
            }

            KeyCode::Char('R') => self.refresh_now(),
            KeyCode::Char('L') if self.focus == Focus::Inbox => self.refresh(true),
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
            KeyCode::Char('y') if self.focus == Focus::Messages => {
                if let Some(message) = self.selected_message() {
                    self.retry_outgoing(&message.id);
                }
            }
            KeyCode::Char('D') if self.focus == Focus::Messages => {
                if let Some(message) = self.selected_message().filter(Message::is_local_outgoing) {
                    self.dismiss_outgoing(&message.id);
                }
            }
            KeyCode::Char('e') => {
                if self.active.as_ref().is_some_and(|c| c.can_react)
                    && self
                        .selected_message()
                        .is_some_and(|m| !m.is_local_outgoing())
                {
                    self.mode = Mode::React;
                    self.reaction = 0
                } else {
                    self.notice = "This connection does not support emoji reactions.".into()
                }
            }
            KeyCode::Char('E') => {
                if let Some(m) = self.selected_message()
                    && m.is_sender
                    && !m.is_local_outgoing()
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
    #[test]
    fn pinned_chats_lead_their_tab_in_dragged_order_and_unpin_back_to_recency() {
        let mut app = App::new(None, true);
        app.source = 0;
        let ids: Vec<String> = app.visible().iter().map(|c| c.id.clone()).collect();
        assert!(ids.len() >= 4);
        let order = |app: &App| {
            app.visible()
                .iter()
                .map(|c| c.id.clone())
                .collect::<Vec<_>>()
        };
        app.move_pin(&ids[2], Some(0));
        app.move_pin(&ids[3], Some(0));
        assert_eq!(order(&app)[..2], [ids[3].clone(), ids[2].clone()]);
        assert_eq!(app.pinned_count(), 2);
        app.move_pin(&ids[2], Some(0));
        assert_eq!(order(&app)[..2], [ids[2].clone(), ids[3].clone()]);
        app.move_pin(&ids[2], None);
        let mut rest = ids.clone();
        rest.retain(|id| *id != ids[3]);
        assert_eq!(order(&app), [vec![ids[3].clone()], rest].concat());
    }
    #[test]
    fn pins_from_all_accounts_carry_into_each_app_tab() {
        let mut app = App::new(None, true);
        let slack = SOURCES.iter().position(|s| s.0 == "slack").unwrap();
        let chat = app
            .conversations
            .iter()
            .find(|c| c.source == "slack")
            .unwrap()
            .id
            .clone();
        app.source = 0;
        app.move_pin(&chat, Some(0));
        app.source = slack;
        assert!(app.is_pinned(&chat));
        assert_eq!(app.visible()[0].id, chat);
        app.move_pin(&chat, None);
        assert!(!app.is_pinned(&chat));
        app.source = 0;
        assert!(!app.is_pinned(&chat));
        app.source = slack;
        app.move_pin(&chat, Some(0));
        app.source = 0;
        assert!(!app.is_pinned(&chat));
        app.move_pin(&chat, Some(0));
        app.move_pin(&chat, None);
        app.source = slack;
        assert!(app.is_pinned(&chat));
    }

    use super::*;
    #[test]
    fn refreshing_a_paged_inbox_keeps_a_row_displaced_from_the_first_page() {
        let mut app = App::new(None, true);
        app.source = 0;
        app.conversations = vec![
            Conversation {
                id: "first".into(),
                timestamp: "2026-11-22T12:00:00Z".into(),
                ..Default::default()
            },
            Conversation {
                id: "displaced".into(),
                timestamp: "2026-11-22T11:00:00Z".into(),
                ..Default::default()
            },
            Conversation {
                id: "older".into(),
                timestamp: "2026-11-22T10:00:00Z".into(),
                ..Default::default()
            },
        ];
        app.inbox_keep_tail = true;
        app.inbox_cursors = json!({"matrix":"older-cursor"});
        app.tx.send(Response::Inbox(app.request_seq, Ok(json!({
            "items":[{"id":"new","timestamp":"2026-11-22T13:00:00Z"},{"id":"first","timestamp":"2026-11-22T12:00:00Z"}],
            "cursors":{"matrix":"new-cursor"}
        })),false)).unwrap();
        app.poll();
        assert_eq!(
            app.conversations
                .iter()
                .map(|chat| chat.id.as_str())
                .collect::<Vec<_>>(),
            ["new", "first", "displaced", "older"]
        );
        assert_eq!(app.inbox_cursors["matrix"], "older-cursor");
    }
    #[test]
    fn mute_stops_counting_as_unread_immediately() {
        let mut app = App::new(None, true);
        let mut chat = app.conversations[0].clone();
        chat.unread = true;
        chat.unread_count = 4;
        let id = chat.id.clone();
        app.conversations[0] = chat.clone();
        app.active = Some(chat);
        app.apply_channel_rule(&id, Some("important"), None);
        assert!(app.conversations[0].unread);
        assert_eq!(app.conversations[0].local_priority, "important");
        app.apply_channel_rule(&id, None, Some(Some(60)));
        assert!(app.conversations[0].mute_until.is_some());
        assert!(!app.conversations[0].unread);
        assert!(!app.active.as_ref().unwrap().unread);
        assert_eq!(app.conversations[0].unread_count, 0);
        assert_eq!(app.conversations[0].local_priority, "important");
        app.apply_channel_rule(&id, Some("auto"), Some(None));
        assert!(app.conversations[0].mute_until.is_none());
        assert_eq!(app.conversations[0].local_priority, "auto");
    }
    #[test]
    fn unread_opening_keeps_previous_boundary_after_optimistic_read() {
        let mut app = App::new(None, true);
        let mut chat = app.active.clone().unwrap();
        chat.unread = true;
        chat.local_read_at = Some(
            chrono::DateTime::parse_from_rfc3339("2026-11-11T10:00:00Z")
                .unwrap()
                .timestamp_millis(),
        );
        app.open(chat);
        assert!(!app.active.as_ref().unwrap().unread);
        app.messages = serde_json::from_value(json!([
            {"id":"read","timestamp":"2026-11-11T09:59:00Z"},
            {"id":"mine","timestamp":"2026-11-11T10:00:30Z","isSender":true},
            {"id":"first-new","timestamp":"2026-11-11T10:01:00Z"},
            {"id":"later","timestamp":"2026-11-11T10:02:00Z"}
        ]))
        .unwrap();
        app.locate_first_unread();
        assert_eq!(app.first_unread.as_deref(), Some("first-new"));
        assert_eq!(app.message_selection, 2);
        app.first_unread.take();
        app.locate_first_unread();
        assert!(
            app.first_unread.is_none(),
            "background refresh must not jump again"
        );
    }
    #[test]
    fn unread_opening_loads_back_to_the_boundary_and_falls_back_to_counts() {
        let mut app = App::new(None, true);
        app.opening_unread = Some((Some(1000), 1));
        app.messages = vec![Message {
            id: "new".into(),
            timestamp: "1970-01-01T00:00:02Z".into(),
            ..Default::default()
        }];
        app.message_cursor = Some("older".into());
        app.locate_first_unread();
        assert!(app.first_unread.is_none());
        assert!(app.opening_unread.is_some());
        app.message_cursor = None;
        app.locate_first_unread();
        assert_eq!(app.first_unread.as_deref(), Some("new"));
        app.first_unread = None;
        app.opening_unread = Some((None, 3));
        app.message_cursor = Some("older".into());
        app.locate_first_unread();
        assert!(app.first_unread.is_none());
        app.messages.insert(
            0,
            Message {
                id: "first".into(),
                ..Default::default()
            },
        );
        app.messages.insert(
            1,
            Message {
                id: "second".into(),
                ..Default::default()
            },
        );
        app.locate_first_unread();
        assert_eq!(app.first_unread.as_deref(), Some("first"));
    }
    #[test]
    fn stale_inbox_cannot_restore_unread_but_new_messages_can_after_sync() {
        let mut app = App::new(None, true);
        let mut chat = app.active.clone().unwrap();
        chat.unread = true;
        app.conversations = vec![chat.clone()];
        let (queue, mut reads) = unbounded_channel();
        app.read_tx = Some(queue);
        app.set_read_state(true);
        let (id, revision, _) = reads.try_recv().unwrap();
        app.tx
            .send(Response::ReadState(id, revision, true, Ok(json!({}))))
            .unwrap();
        app.poll();
        for (seq, source_unread, displayed_unread) in
            [(1, true, false), (2, false, false), (3, true, true)]
        {
            app.request_seq = seq;
            chat.unread = source_unread;
            app.tx
                .send(Response::Inbox(seq, Ok(json!({"items":[chat]})), false))
                .unwrap();
            app.poll();
            assert_eq!(app.conversations[0].unread, displayed_unread);
        }
    }
    #[test]
    fn opening_clears_unread_before_messages_or_receipts_arrive() {
        let mut app = App::new(None, true);
        let mut chat = app.active.clone().unwrap();
        chat.unread = true;
        app.conversations = vec![chat.clone()];
        app.messages.clear();
        app.message_cache.clear();
        app.open(chat.clone());
        assert!(!app.active.as_ref().unwrap().unread);
        assert!(!app.conversations[0].unread);
        assert_eq!(app.pending_read.as_deref(), Some(chat.id.as_str()));
        app.tx
            .send(Response::Inbox(
                app.request_seq,
                Ok(json!({"items":[chat], "_cache":{"cached":true}})),
                false,
            ))
            .unwrap();
        app.poll();
        assert!(!app.conversations[0].unread);
    }
    #[test]
    fn read_receipts_cannot_undo_newer_manual_unread_and_failures_roll_back() {
        let mut app = App::new(None, true);
        let (queue, mut reads) = unbounded_channel();
        app.read_tx = Some(queue);
        let chat = app.active.clone().unwrap();
        app.conversations = vec![chat.clone()];
        app.set_read_state(true);
        let (_, old_revision, _) = reads.try_recv().unwrap();
        app.set_read_state(false);
        let (_, revision, _) = reads.try_recv().unwrap();
        assert!(app.conversations[0].unread);
        app.tx
            .send(Response::ReadState(
                chat.id.clone(),
                old_revision,
                true,
                Ok(json!({})),
            ))
            .unwrap();
        app.poll();
        assert!(app.conversations[0].unread);
        app.tx
            .send(Response::ReadState(
                chat.id,
                revision,
                false,
                Err("offline".into()),
            ))
            .unwrap();
        app.poll();
        assert!(!app.conversations[0].unread);
        assert!(app.notice.contains("offline"));
    }
    #[test]
    fn selected_mentions_keep_their_identity_through_edits_before_the_tag() {
        let mut app = App::new(None, true);
        app.editor.set("@vi".into());
        app.editor
            .replace_completion(0..3, "@Victoria ", Some("@discord_123:chord.local".into()));
        let old = app.editor.text.clone();
        app.editor.text.insert_str(0, "😀 <hi> ");
        app.editor.reconcile_mentions(&old);
        app.prepare_send();
        let body = app.confirm_body.as_ref().unwrap();
        assert_eq!(
            body["mentions"],
            json!({"userIds": ["@discord_123:chord.local"]})
        );
        assert_eq!(
            body["html"],
            "😀 &lt;hi&gt; <a href=\"https://matrix.to/#/%40discord_123%3Achord.local\">@Victoria</a> "
        );

        let old = app.editor.text.clone();
        app.editor.text = old.replace("@Victoria", "@Victor");
        app.editor.reconcile_mentions(&old);
        assert!(app.editor.mention_payload().is_none());
    }

    #[test]
    fn native_send_submits_without_a_review_dialog() {
        let mut app = App::new(None, true);
        app.native_ui = true;
        app.editor.set("Test draft".into());
        app.prepare_send();
        assert_eq!(app.mode, Mode::Browse);
        assert!(app.confirm_body.is_none());
        assert!(app.editor.text.is_empty());
        assert!(
            app.messages
                .iter()
                .any(|message| message.text == "Test draft" && message.is_sender)
        );
        assert!(!app.notice.contains("Submitting"));
        assert!(app.api.is_none());
    }
    #[test]
    fn inline_reply_loading_preserves_channel_and_older_history_cursor() {
        let mut app = App::new(None, true);
        let chat_id = app.active.as_ref().unwrap().id.clone();
        app.message_cursor = Some("older-channel-page".into());
        let before = app.messages.len();
        app.tx.send(Response::InlineThread(chat_id.clone(), Ok(json!({"items":[{"id":"inline-reply","timestamp":"2026-11-10T01:00:00Z","threadRootID":"demo-1"}]})))).unwrap();
        app.poll();
        assert_eq!(app.messages.len(), before + 1);
        assert_eq!(app.active.as_ref().unwrap().id, chat_id);
        assert_eq!(app.message_cursor.as_deref(), Some("older-channel-page"));
        app.tx
            .send(Response::InlineThread(
                "another-channel".into(),
                Ok(json!({"items":[{"id":"wrong-reply"}]})),
            ))
            .unwrap();
        app.poll();
        assert!(!app.messages.iter().any(|m| m.id == "wrong-reply"));
    }
    #[test]
    fn late_send_and_upload_results_cannot_change_another_conversations_composer() {
        let mut a = App::new(None, true);
        a.active = Some(Conversation {
            id: "second".into(),
            ..Default::default()
        });
        a.editor.set("Keep this unsent text".into());
        a.send_busy = true;
        a.tx.send(Response::Sent(
            "first".into(),
            "missing".into(),
            Ok(json!({})),
        ))
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
    fn failed_send_survives_switching_with_a_newer_draft_and_can_be_retried() {
        let mut app = App::new(None, true);
        app.native_ui = true;
        let chat = app.active.clone().unwrap();
        app.editor.set("Keep me".into());
        let token = app.prepare_send().unwrap();
        app.editor.set("Newer draft".into());
        app.open(Conversation {
            id: "other".into(),
            ..Default::default()
        });
        app.message_cache.remove(&chat.id);
        app.tx
            .send(Response::Sent(
                chat.id.clone(),
                token.clone(),
                Err("offline".into()),
            ))
            .unwrap();
        app.poll();
        app.open(chat);
        assert_eq!(app.editor.text, "Newer draft");
        assert!(app.messages.iter().any(|message| message.id == token
            && message.text == "Keep me"
            && message.send_error.as_deref() == Some("offline")));
        app.retry_outgoing(&token);
        assert_eq!(app.editor.text, "Newer draft");
        assert!(app.messages.iter().any(
            |message| message.id == token && message.send_status.as_deref() == Some("Sending…")
        ));
    }

    #[test]
    fn accepted_send_without_a_provider_id_does_not_match_identical_text() {
        let mut app = App::new(None, true);
        app.native_ui = true;
        let chat = app.active.clone().unwrap();
        app.messages.push(Message {
            id: "earlier".into(),
            text: "thanks".into(),
            is_sender: true,
            timestamp: now_stamp(),
            ..Default::default()
        });
        app.editor.set("thanks".into());
        let token = app.prepare_send().unwrap();
        app.tx
            .send(Response::Sent(chat.id, token.clone(), Ok(json!({}))))
            .unwrap();
        app.poll();
        assert!(app.messages.iter().any(|message| message.id == "earlier"));
        assert!(app.messages.iter().any(|message| message.id == token
            && message.send_status.as_deref() == Some("Sent · awaiting sync")));
        app.dismiss_outgoing(&token);
        assert!(!app.messages.iter().any(|message| message.id == token));
    }

    #[test]
    fn send_stays_visible_when_a_reload_has_not_caught_up() {
        let mut app = App::new(None, false);
        app.native_ui = true;
        app.active = Some(Conversation {
            id: "room".into(),
            kind: "chat".into(),
            can_send: true,
            ..Default::default()
        });
        app.messages.clear();
        app.editor.set("On my way".into());
        app.prepare_send();
        let token = app.outgoing[0].token.clone();
        app.message_request_seq = 1;
        app.tx
            .send(Response::Messages(
                "room".into(),
                1,
                Ok(json!({"items":[{"id":"older","timestamp":"2026-11-18T00:00:00.000Z","text":"earlier"}]})),
                false,
                false,
            ))
            .unwrap();
        app.poll();
        assert!(
            app.messages.iter().any(|message| message.id == token),
            "a stale page must not hide the message that is still in flight"
        );
        app.tx
            .send(Response::Sent(
                "room".into(),
                token.clone(),
                Ok(json!({"pendingMessageID":"$server"})),
            ))
            .unwrap();
        app.poll();
        assert!(
            app.outgoing
                .iter()
                .any(|item| item.server_id.as_deref() == Some("$server"))
        );
        app.message_request_seq = 2;
        app.tx
            .send(Response::Messages(
                "room".into(),
                2,
                Ok(json!({"items":[
                    {"id":"older","timestamp":"2026-11-18T00:00:00.000Z","text":"earlier"},
                    {"id":"$server","timestamp":"2026-11-18T01:00:00.000Z","text":"On my way","isSender":true}
                ]})),
                false,
                false,
            ))
            .unwrap();
        app.poll();
        assert!(app.outgoing.is_empty());
        assert!(!app.messages.iter().any(|message| message.id == token));
        assert!(app.messages.iter().any(|message| message.id == "$server"));
    }
    #[test]
    fn reaction_shows_immediately_and_survives_a_stale_reload() {
        let mut app = App::new(None, false);
        app.active = Some(Conversation {
            id: "room".into(),
            can_react: true,
            ..Default::default()
        });
        app.status = json!({"userID":"@max:chord.local"});
        app.messages = vec![Message {
            id: "m".into(),
            timestamp: "2026-11-18T00:00:00.000Z".into(),
            reactions: vec![
                json!({"reactionKey":"👍","participantID":"sam","participantName":"Sam"}),
            ],
            ..Default::default()
        }];
        app.queue_reaction("room", "m", "👍", true);
        assert!(user_has_reaction(
            &app.messages[0].reactions,
            "👍",
            "@max:chord.local"
        ));
        let revision = app.pending_reactions[0].sent.unwrap().0;
        app.message_request_seq = 1;
        app.tx
            .send(Response::Messages(
                "room".into(),
                1,
                Ok(json!({"items":[{"id":"m","timestamp":"2026-11-18T00:00:00.000Z","reactions":[{"reactionKey":"👍","participantID":"sam","participantName":"Sam"}]}]})),
                false,
                false,
            ))
            .unwrap();
        app.poll();
        assert!(
            user_has_reaction(&app.messages[0].reactions, "👍", "@max:chord.local"),
            "the click stays visible while the request is still out"
        );
        app.tx
            .send(Response::Reaction(revision, Err("offline".into())))
            .unwrap();
        app.poll();
        assert!(!user_has_reaction(
            &app.messages[0].reactions,
            "👍",
            "@max:chord.local"
        ));
        assert!(
            app.messages[0]
                .reactions
                .iter()
                .any(|reaction| reaction["participantName"] == "Sam")
        );
        assert!(app.notice.contains("offline"));
    }
    #[test]
    fn a_later_reaction_click_wins_over_an_older_response() {
        let mut app = App::new(None, false);
        app.active = Some(Conversation {
            id: "room".into(),
            ..Default::default()
        });
        app.messages = vec![Message {
            id: "m".into(),
            ..Default::default()
        }];
        app.queue_reaction("room", "m", "❤️", true);
        let first = app.pending_reactions[0].sent.unwrap().0;
        app.queue_reaction("room", "m", "❤️", false);
        assert!(app.pending_reactions[0].sent.is_some());
        assert!(!app.pending_reactions[0].desired);
        app.tx
            .send(Response::Reaction(first, Ok(json!({"success": true}))))
            .unwrap();
        app.poll();
        assert!(!user_has_reaction(&app.messages[0].reactions, "❤️", ""));
        assert!(app.pending_reactions[0].sent.is_some());
        assert!(!app.pending_reactions[0].desired);
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
        assert_eq!(
            reads.try_recv().map(|(id, _, read)| (id, read)).unwrap(),
            (chat.id.clone(), true)
        );
        assert!(app.pending_read.is_none());
        let mut next = chat.clone();
        next.id = "matrix:other".into();
        app.open(next);
        assert!(reads.try_recv().is_err());
        app.open(chat.clone());
        assert_eq!(
            reads.try_recv().map(|(id, _, read)| (id, read)).unwrap(),
            (chat.id, true)
        );
        app.set_read_state(false);
        assert!(!reads.try_recv().map(|(id, _, read)| (id, read)).unwrap().1);
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
        assert_eq!(
            reads.try_recv().map(|(id, _, read)| (id, read)).unwrap(),
            (chat.id.clone(), true)
        );
        app.set_read_state(false);
        assert_eq!(
            reads.try_recv().map(|(id, _, read)| (id, read)).unwrap(),
            (chat.id.clone(), false)
        );
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
        assert_eq!(
            reads.try_recv().map(|(id, _, read)| (id, read)).unwrap(),
            (chat.id.clone(), false)
        );
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
    fn unimportant_visibility_and_unread_filters_are_independent_of_pins() {
        let mut app = App::new(None, true);
        app.source = 0;
        let hidden = app.conversations[0].id.clone();
        app.conversations[0].unread = true;
        app.move_pin(&hidden, Some(0));
        app.set_unimportant(&hidden, true);

        assert!(app.visible().iter().all(|chat| chat.id != hidden));
        assert!(app.is_pinned(&hidden));
        app.show_unimportant = true;
        app.unread = true;
        assert_eq!(app.visible()[0].id, hidden);
        assert!(app.visible().iter().all(|chat| chat.unread));
        app.set_unimportant(&hidden, false);
        app.show_unimportant = false;
        assert_eq!(app.visible()[0].id, hidden);
    }

    #[test]
    fn discord_server_visibility_spares_dms_and_respects_show_unimportant() {
        let mut app = App::new(None, true);
        app.conversations = vec![
            Conversation {
                id: "channel".into(),
                source: "discord".into(),
                server_id: Some("123".into()),
                ..Default::default()
            },
            Conversation {
                id: "dm".into(),
                source: "discord".into(),
                ..Default::default()
            },
        ];

        assert_eq!(app.visible().len(), 2);
        app.unimportant_discord_servers.insert("123".into());
        assert_eq!(app.visible()[0].id, "dm");
        assert_eq!(app.visible().len(), 1);
        app.show_unimportant = true;
        assert_eq!(app.visible().len(), 2);
    }

    #[test]
    fn selecting_a_discord_server_keeps_show_unimportant_setting() {
        let mut app = App::new(None, true);
        assert!(!app.show_unimportant);
        app.show_unimportant = true;
        app.servers = vec![json!({"id":"123","name":"SDC Buggy","channelCount":23})];
        app.unread = true;
        app.key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Servers);
        app.paste("sdc");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.source, 2);
        assert_eq!(app.discord_scope.as_deref(), Some("123"));
        assert!(!app.unread);
        assert!(app.show_unimportant);
        app.key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert!(app.discord_scope.is_none());
        assert!(app.show_unimportant);
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
