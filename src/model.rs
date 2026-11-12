use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub remote_id: String,
    pub title: String,
    pub preview: String,
    pub preview_sender_name: String,
    pub timestamp: String,
    pub source: String,
    pub account_key: String,
    pub account_name: Option<String>,
    #[serde(rename = "serverID")]
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub kind: String,
    pub unread: bool,
    pub unread_count: usize,
    pub local_read_at: Option<i64>,
    #[serde(rename = "threadRootID")]
    pub thread_root_id: Option<String>,
    pub parent_conversation_id: Option<String>,
    pub can_send: bool,
    pub can_reply: bool,
    pub can_react: bool,
    pub participants: Vec<String>,
    pub blue: Value,
    pub direct: bool,
    pub local_priority: String,
    pub mute_until: Option<i64>,
    #[serde(rename = "contactIDs")]
    pub contact_ids: Vec<String>,
    pub avatar_contacts: Vec<Value>,
    pub importance: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Message {
    pub historical_version: bool,
    pub id: String,
    pub text: String,
    pub sender_name: String,
    #[serde(rename = "senderContactID")]
    pub sender_contact_id: Option<String>,
    pub timestamp: String,
    pub is_sender: bool,
    pub linked_message_id: Option<String>,
    #[serde(rename = "threadRootID")]
    pub thread_root_id: Option<String>,
    pub reply_count: usize,
    pub reactions: Vec<Value>,
    pub attachments: Vec<Value>,
    pub to: String,
    pub from: String,
    pub subject: String,
    pub message_id_header: String,
    pub references: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Inbox {
    pub items: Vec<Conversation>,
    pub errors: Vec<Value>,
    pub cursors: Value,
    pub summary: Value,
}

pub const SOURCES: &[(&str, &str, &str)] = &[
    ("all", "All accounts", "1"),
    ("slack", "Slack", "2"),
    ("discord", "Discord", "3"),
    ("whatsapp", "WhatsApp", "4"),
    ("iphone", "iPhone", "5"),
    ("mail:home", "Personal mail", "6"),
    ("mail:cmu", "CMU mail", "7"),
];

pub fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

pub fn one_line(text: &str) -> String {
    clean(text).split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn source_label(source: &str) -> &str {
    match source {
        "slack" => "slack",
        "discord" => "discord",
        "whatsapp" => "whatsapp",
        "iphone" => "iphone",
        "mail:home" => "personal",
        "mail:cmu" => "cmu",
        _ => "matrix",
    }
}

pub fn day_label(stamp: &str) -> String {
    day_label_in(stamp, &chrono::Local)
}

fn day_label_in<Tz: chrono::TimeZone>(stamp: &str, timezone: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    chrono::DateTime::parse_from_rfc3339(stamp)
        .map(|d| d.with_timezone(timezone).format("%A, %B %-d").to_string())
        .unwrap_or_default()
}

pub fn time_label(stamp: &str) -> String {
    let Ok(d) = chrono::DateTime::parse_from_rfc3339(stamp) else {
        return String::new();
    };
    let d = d.with_timezone(&chrono::Local);
    if d.date_naive() == chrono::Local::now().date_naive() {
        d.format("%-I:%M %p").to_string()
    } else {
        d.format("%b %d").to_string()
    }
}

pub fn demo() -> (Vec<Conversation>, Vec<Message>) {
    let now = chrono::Local::now().to_rfc3339();
    let entries = [
        (
            "slack",
            "# teaching",
            "Victoria · Can you review the session plan? 🎉",
            true,
        ),
        (
            "discord",
            "# weekend-rides",
            "Sam · Saturday morning works for me",
            true,
        ),
        (
            "whatsapp",
            "The dinner table",
            "Alex · Bringing something good",
            false,
        ),
        (
            "mail:cmu",
            "Lab meeting notes",
            "Research group · This week's agenda",
            true,
        ),
        ("iphone", "Friends", "Jamie · See you there", false),
        (
            "mail:home",
            "Your next tutoring session",
            "Student · Tuesday at 6?",
            false,
        ),
    ];
    let chats = entries
        .iter()
        .enumerate()
        .map(|(i, (s, t, p, u))| Conversation {
            id: format!("demo:{i}"),
            title: (*t).into(),
            preview: (*p).into(),
            timestamp: now.clone(),
            source: (*s).into(),
            kind: if s.starts_with("mail") {
                "email"
            } else {
                "chat"
            }
            .into(),
            unread: *u,
            can_send: true,
            can_reply: true,
            can_react: !s.starts_with("mail"),
            ..Default::default()
        })
        .collect();
    let messages = vec![
        Message{id:"demo-1".into(),sender_name:"Victoria".into(),timestamp:now.clone(),text:"I've put together the plan for next week's review.\n\nCould you take a look at the examples before we share it?".into(),..Default::default()},
        Message{id:"demo-2".into(),sender_name:"You".into(),is_sender:true,timestamp:now.clone(),text:"Sure. The first example looks good. Let's leave a little more time for questions.".into(),reactions:vec![serde_json::json!({"reactionKey":"👍","participantID":"demo-person"})],..Default::default()},
        Message{id:"demo-3".into(),sender_name:"Victoria".into(),timestamp:now,text:"Sounds good. I'll update the timing.\n\nThanks for looking at it! 🎉 ❤️ 👍🏽 🇺🇸 👩‍💻".into(),linked_message_id:Some("demo-2".into()),..Default::default()},
    ];
    (chats, messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_day_uses_the_local_calendar_date_across_utc_midnight() {
        let eastern_winter = chrono::FixedOffset::west_opt(5 * 3600).unwrap();
        for (stamp, expected) in [
            ("2026-11-09T01:19:29Z", "Sunday, November 8"),
            ("2026-11-08T20:19:29-05:00", "Sunday, November 8"),
            ("2026-11-09T04:59:59Z", "Sunday, November 8"),
            ("2026-11-09T05:00:00Z", "Monday, November 9"),
            ("2027-01-01T01:00:00Z", "Thursday, December 31"),
        ] {
            assert_eq!(day_label_in(stamp, &eastern_winter), expected);
        }
        let eastern_summer = chrono::FixedOffset::west_opt(4 * 3600).unwrap();
        assert_eq!(
            day_label_in("2026-07-10T03:59:59Z", &eastern_summer),
            "Thursday, July 9"
        );
        assert_eq!(
            day_label_in("2026-07-10T04:00:00Z", &eastern_summer),
            "Friday, July 10"
        );
        let east_of_utc = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
        assert_eq!(
            day_label_in("2026-11-08T23:00:00Z", &east_of_utc),
            "Monday, November 9"
        );
    }

    #[test]
    fn invalid_message_dates_have_no_day_heading() {
        assert_eq!(day_label_in("not a timestamp", &chrono::Utc), "");
        assert_eq!(day_label_in("", &chrono::Utc), "");
    }
}
