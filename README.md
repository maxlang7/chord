# Chord

A Rust terminal client for independent chat and email, with Gruvbox Dark enabled.
The local adapter connects an owned Matrix server and network bridges, Gmail,
and BlueFerry. It does not require a Beeper account or running Beeper app.

Launch **Chord** from the application menu to open its Ghostty window. The
desktop launcher starts the project-owned local runtime and API when needed and
reuses them when they are already running. Its entry is
`~/.local/share/applications/chord.desktop`; its source template is
`packaging/chord.desktop.in`.

This checkout is under active development. For an incremental client preview
with the backend already running:

```sh
nix-shell shell.nix --run 'cargo run'
```

## Discord servers

Press `3` or `s` to open the server picker. Type a server name, use the arrow
keys to select it, and press Enter. Choosing a server shows all its imported
channels, with the server name visible in the sidebar and message header.
Direct messages and All Discord are separate picker entries. `s` switches to
another server. `v` can turn Focus on within the selected server; returning to
another account restores the global Focus setting.

Open chat conversations refresh automatically every five seconds while browsing,
keeping your reading position and loaded history. Composing a message is left
undisturbed.

## Images

Select an image message in the message pane (`Tab`, then `j`/`k`) to see an
inline preview. Press `o` for the larger gallery, then:

- Left/right arrows browse images attached to that message.
- `d` saves the original in `private/downloads/` without overwriting existing files.
- `b` saves and opens the image in Brave.
- `R` retries a failed download; `Esc` returns to the conversation.

PNG, JPEG, WebP, BMP, and the first frame of GIF images are supported. Native
terminal graphics are detected at startup, with a colored half-block fallback.
Set `CHORD_IMAGE_PROTOCOL=halfblocks` to force that fallback. The viewer loads
through the authenticated local API, including encrypted chat attachments and
Gmail image attachments; it never fetches remote email tracking images. BlueFerry
currently supplies text only. Images above 20 MB or decoder limits show an error.

Press `f` to attach a file to a chat. Recognized image files are sent with their
image MIME type. Sending still requires the usual review and confirmation.
AI clients can use `download_chat_attachment` via MCP, or
`POST /api/conversations/{id}/attachment` with `messageID` and zero-based `index`.

## Contacts

Names come from Slack and Discord profiles, WhatsApp contacts, and the iPhone
address book exposed by BlueFerry. Known names are used in message headers,
direct-conversation titles, and recognized mentions. When a source has no usable
name, the contact is marked as needing a name; no real name is invented.

Press `c` to open contacts. While reading messages, `c` opens the selected
sender's contact. `C` always opens the directory. Type to search, use the arrow
keys to choose a contact, and press Enter to open it.

In a contact:

- `e` edits the name used throughout Chord.
- `*` toggles favorite status and gives their direct conversations priority.
- `m` links another account after a review screen. Matching display names never
  cause automatic links. Exact source-provided phone/email identifiers can link.
- `u` undoes the last explicit link, provided later edits would not be lost.
- Arrow keys and Enter open a loaded conversation with that contact.

Contact names, links, favorites, and chat rules are saved in the owner-only
`private/contacts.json`. Editing a contact here does not edit network profiles or
the phone's address book. Keep `private/` out of source control and backups you
intend to publish.

## Focus

The client opens in Focus. `v` switches between Focus and All. Searching includes
all matching conversations, including those hidden from Focus.

Focus uses explicit, explained signals: recent direct conversations, unread
personal messages, mentions and replies, conversations you have participated in,
favorite contacts, and starred or important email. Activity comes from actual
message timestamps, so importing old history does not make it appear recent.
Unread channels alone do not qualify. Muted chats, unengaged channels, automated
accounts, and promotional mail are generally kept in All.

Each Focus row gives its reason. You control the exceptions:

- `*` pins the selected conversation in Focus; press again to restore automatic
  importance.
- `z` quiets the selected conversation locally; press again from All to restore it.
- `=` restores automatic importance for the selected conversation.
- `u` toggles the unread filter, independently of Focus/All.

Focus is a local view. It does not archive, delete, mark read, or mute anything on
the source network. Importance is a heuristic, so All remains one key away.

## API and MCP

The loopback API requires the bearer key in `private/config.json`. It rejects
browser cross-origin requests. Contact and importance operations are available
to integrations as well as the TUI:

| Endpoint | Operation |
| --- | --- |
| `GET /api/inbox?view=focus` | Explained importance ranking and hidden/total counts |
| `GET /api/inbox?view=all` | All conversations; default for API callers |
| `GET /api/contacts?query=...` | Search names and network identities |
| `GET /api/contacts/{id}` | Contact details and loaded conversations |
| `POST /api/contacts/{id}` | Set `name` or `favorite` |
| `POST /api/contacts/merge` | Link `sourceID` to `targetID`, with `confirm: true` |
| `POST /api/contacts/undo-merge` | Undo the last explicit link |
| `POST /api/preferences/conversation` | Set `conversationID` and `priority` to `auto`, `important`, or `quiet` |

Run `target/debug/chord --mcp` for stdio MCP. It includes `list_contacts`,
`update_contact`, `link_contacts`, `set_importance`, and the existing messaging
tools. `list_conversations` accepts `view: "focus"` or `view: "all"`. Incoming
message text is never executed or used to trigger outgoing actions automatically.

## Validation

```sh
nix-shell shell.nix --run 'cargo test'
nix-shell -p cargo rustc rustfmt clippy --run 'cargo fmt --check && cargo clippy --all-targets -- -D warnings'
nix-shell shell.nix --run 'node --test test/*.test.mjs'
```

The contact tests cover name persistence, identity separation, explicit links and
undo, sender-name resolution, and Focus rules. Regression checks also cover
room discovery and thread replies in the local engine. Live verification should
read existing conversations; external test messages are unnecessary.
