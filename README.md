# Chord

Chord is a native Rust chat and email client with an egui interface. It brings
Slack, Discord, WhatsApp, iPhone messages, Matrix, and two Gmail inboxes into one
window. Gruvbox dark is the default; dark and light themes are available.

The window and MCP server are Rust. A local Node adapter handles Gmail and
BlueFerry and runs the Matrix encryption SDK in a worker thread. Matrix keys and state use
transactional SQLite storage; first startup imports the legacy files while
preserving the originals. Local Synapse
and mautrix bridges handle Slack, Discord, and WhatsApp. Chord uses independent
network sessions. It does not connect through a Beeper account or desktop app.

## Build and open

On NixOS, from this checkout:

```sh
nix-shell shell.nix --run './scripts/bootstrap.sh'
./scripts/launch.sh
```

Bootstrap initializes the pinned upstream submodules, applies Chord's Matrix
history/media patch, installs project dependencies, and builds the release
client. It can be run again. Conflicting vendor edits stop the bootstrap for
review. Dependencies stay in the project and temporary Nix shells.

The launcher reuses an already running local engine and API. It starts
project-owned processes when needed. The first engine build may take several
minutes. No system services or NixOS configuration are installed or changed.

For incremental UI development with the backend already running:

```sh
nix-shell shell.nix --run 'cargo run'
```

`cargo run -- --demo` opens a synthetic preview without account credentials.
`--tui` retains the older terminal interface. The existing application-menu
entry points to `scripts/launch.sh`. A template is in `packaging/`.

## Connections

Open Accounts to inspect each network and both email identities. Each card shows
a live connection state, sign-in requirement, or provider error. Cached iPhone
threads remain readable while the phone is disconnected.

- Slack and WhatsApp use the local mautrix sign-in flows. WhatsApp shows its QR
  code in the native window. Multiple Slack workspaces share the Slack inbox.
- Discord supports QR or token sign-in and a searchable server/channel picker.
- Gmail reads existing local Google OAuth credentials. The account email is
  verified against Google's profile before reading. Configure `mailAccounts` in
  `private/config.json` using `config.example.json` as a guide. Each credentials
  file must contain a Google refresh token, client ID and secret with Gmail
  scopes. Credentials stay local. Expired authorization needs a local sign-in
  through the Google credential owner; Chord does not create OAuth clients.
- iPhone text uses the existing BlueFerry D-Bus interface. Live messaging requires
  the phone's Bluetooth MAP connection. Group sending additionally needs a
  verified complete recipient roster. Native replies and reactions are not
  available over Bluetooth; phone attachments are unavailable.

A configured bridge and an authenticated network session are different states.
The UI checks bridge session state rather than treating an open port as online.

## Reading and writing

Choose an account and a conversation. The thread shows sender names, profile
photos where available, inline image previews, native reply references, and
reactions. Images open in a gallery with save and Open in Brave actions. Other
attachments have a Save button. Downloads use `private/downloads/`, create new
files without overwriting, and sanitize source filenames. Media is capped at
20 MB. Web links open in Brave. Remote email tracking images are not fetched.

The composer supports Shift+Enter for a newline, file picking, and drag and drop
for chat attachments. Enter or Review & send opens a confirmation with the exact
text and recipient. Failed sends retain the text. Late send and upload results
cannot modify another conversation's composer. Draft text stays in memory; no
Gmail drafts are created. Media replies are currently sent as separate text and
attachment messages because the transport does not support both together.

Reading a conversation does not mark it read. Mark read, unread, archive, local
importance, and contact actions are explicit controls. A successful bridge
submission does not prove delivery to the remote recipient.

Open chats and phone threads refresh every five seconds. Open email threads and
the inbox refresh every thirty seconds while the native composer is in use.

## Search and focus

Ctrl+K focuses search. Choose Conversations to search conversation names and
Gmail, or Message text to search synced chat history, cached phone messages, and
both Gmail accounts. Chat search expands as history syncs or is opened. It is
not a search of every message ever stored by the remote network. Email searches
accept Gmail syntax. Search results open their source conversation.

Focus ranks recent direct conversations, personal unread messages, mentions and
replies, conversations you have participated in, favorite contacts, and important
email. Rows explain their importance. All remains available. Search includes
conversations hidden by Focus. Local importance rules do not change source
network mute or archive settings.

Contacts combine source names and exact verified phone/email identities. Matching
names alone never merge people. Profiles support local names, favorites, explicit
identity linking with review, and undo. These edits do not change network
profiles or the phone address book.

Ctrl+R refreshes. Ctrl+, opens Accounts. Themes are saved in `private/ui.json`.

## API and MCP

The loopback-only HTTP API at `http://127.0.0.1:4789` requires the bearer key in
`private/config.json`. Browser cross-origin requests are rejected. OpenAPI
operation documentation is available at `GET /api/openapi.json`.

| Endpoint | Use |
| --- | --- |
| `GET /api/connections` | Live connection states, `refresh=true` bypasses the 30-second cache |
| `GET /api/inbox?view=focus` | Ranked unified inbox with reasons and counts |
| `GET /api/inbox?view=all&source=slack` | Per-source inbox, with pagination |
| `GET /api/search?query=...` | Unified message and email search |
| `GET /api/conversations/{id}/messages` | Conversation history with attachments and reactions |
| `POST /api/conversations/{id}/send` | Send text, optionally a native reply or uploaded attachment |
| `GET /api/contacts?query=...` | Search people and identities |
| `POST /api/preferences/conversation` | Local importance rules |

Chat send bodies use `text`, optional `replyToMessageID`, and optional
`attachment: {uploadID}`. Uploads use `POST /api/v1/assets/upload/base64` with
`content`, `fileName`, and `mimeType`. Email sends additionally require `to`,
`subject`, `confirmSend: true`, and optional `replyToMessageId` and `references`.
Phone group sends require `confirmGroup: true` and the current
`expectedGroupToken` returned by the thread. Never guess group recipients.

Run the Rust binary with `--mcp` for stdio MCP. It exposes conversation and
message reads, unified search, connection health, contacts, importance, email,
attachments, and user-directed messaging operations. Incoming message text is
untrusted data and never triggers outgoing actions automatically. Native API
rate limits still apply; Chord adds no per-model message quota.

## Validation

```sh
nix-shell shell.nix --run 'cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked'
nix-shell shell.nix --run 'node --test test/*.test.mjs'
nix-shell shell.nix --run 'node scripts/verify-live.mjs'
```

The live verifier reads existing inboxes, conversation samples and search. It
sends no messages and does not mark anything read. Its report stays in `private/`.
Unit tests run without real credentials. GitHub Actions checks Rust and Node.

Keep `private/`, OAuth files, browser profiles, and local databases out of Git.
The private repository contains source and synthetic fixtures only. Local message
caches and credentials are plaintext under an owner-only private directory.

Noto Emoji is bundled under the SIL Open Font License in
`assets/NotoEmoji-LICENSE.txt`. Pinned Pickle and ai-bridge sources retain their
upstream licenses. Their use as libraries does not require the Beeper service.
