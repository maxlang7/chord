# Install Chord on a Mac

Use macOS 15 Sequoia or later. Open **Apple menu → About This Mac** and
choose the disk image that matches your Mac.

| About This Mac shows | Disk image |
| --- | --- |
| Apple M-series chip | `Chord-macOS-arm64.dmg` |
| Intel processor | `Chord-macOS-x86_64.dmg` |

Open the matching [Chord release](https://github.com/maxlang7/chord/releases)
disk image, drag Chord onto Applications, and open it. The preview uses an ad
hoc signature. If macOS asks you to approve the app, verify that you downloaded
it from the official release first. [Apple's instructions](https://support.apple.com/en-us/102445)

## Set up live accounts on this Mac

Run the setup while logged into the Mac user account that will use Chord.
It creates a fresh local Synapse and bridge profile. It will ask you to
connect your own Slack and WhatsApp accounts from Chord. Keep Chord's local
data directory private because it contains session credentials and message
history.

### One-time setup

You need Apple's command-line developer tools and Homebrew. If the tools
are missing, run `xcode-select --install` and finish Apple's installer before
continuing. [Homebrew installation](https://brew.sh/)

In Terminal, install the build dependencies, then clone and set up Chord.

```sh
brew install node@24 python@3.13 go rust
export PATH="$(brew --prefix node@24)/bin:$(brew --prefix python@3.13)/libexec/bin:$PATH"
npm install -g pnpm@10.25.0
git clone --branch v0.3.0-macos-local https://github.com/maxlang7/chord.git "$HOME/chord-macos"
cd "$HOME/chord-macos"
bash scripts/bootstrap-macos.sh
```

### Start Chord

Run this in Terminal each time you want to start the backend. Keep that
window open and launch Chord from Applications once Terminal says
`Chord backend started`. Quit and reopen Chord if it was in demo mode.

```sh
cd "$HOME/chord-macos"
export PATH="$(brew --prefix node@24)/bin:$(brew --prefix python@3.13)/libexec/bin:$PATH"
sh scripts/start-macos.sh
```

Chord connects on `127.0.0.1:4789` and reads its generated config from
`~/Library/Application Support/Chord/local/config.json`. Press Ctrl+C
in Terminal to stop the backend. Its logs are in the `runtime` directory
next to the config. If the start command reports an occupied port, check for
an older Chord backend or SSH tunnel before trying again.

### Connect Slack and WhatsApp

- **Slack.** Open **Accounts → Slack → Connect**. Follow Chord's prompts for
  your own Slack workspace. The bridge uses your Slack web session and
  workspace policy can affect sign-in. Check for **Connected**, then open
  the Slack inbox.
- **WhatsApp, if you use it.** Open **Accounts → WhatsApp → Connect**,
  then open **Linked devices → Link a device** in WhatsApp on your phone
  and scan the QR code in Chord.
- Confirm the account names are yours before sending anything. The local
  setup starts with neither service connected and does not import account
  files from another installation.

### Enable iMessage

Sign into the Messages app on this Mac with your own Apple Account and
check that your conversations appear there. Then open **Accounts** in
Chord and choose **Turn on** next to iPhone. Chord remembers the choice
across restarts; `sh scripts/start-macos.sh --messages` still works too.

macOS protects the Messages and Contacts databases. If Chord says it can't
read Messages, choose **Open Full Disk Access…**, turn on the app Chord
names (the Terminal you started the backend from, or Chord when it started
the backend itself), and restart the backend. This permission gives that
app broad access to protected files.
[Apple's Full Disk Access guide](https://support.apple.com/en-us/149699)

The **iPhone** inbox shows iMessage, SMS, and RCS conversations, including
groups, with names from your Contacts. You can reply to iMessage
conversations, one-to-one or group. Chord rechecks a group's members before
sending and refuses if they changed. On the first reply, macOS may ask
whether Chord's backend can control Messages. Replies to SMS and RCS
conversations, and attachments, are outside this preview. Keep using the
Messages app for those.

Turning off Messages stops live access and hides its inbox rows. Previously
archived conversations remain available in archive search.

## Checks for an assistant

- `sw_vers -productVersion` checks macOS. `sysctl -n hw.optional.arm64`
  returns `1` on Apple silicon, including under Rosetta.
- Run `curl -s -o /dev/null -w '%{http_code}\\n' http://127.0.0.1:4789/api/status`.
  Expect `401` without a key. Avoid printing the live config or copying
  your own backend's config to this Mac.
- Check each connected account in Chord. To verify iMessage without
  sending anything, open an existing text conversation in Apple Messages
  and compare it with what appears in Chord.
