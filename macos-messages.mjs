import {execFile} from 'node:child_process';
import {readdirSync} from 'node:fs';
import {homedir} from 'node:os';
import path from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {promisify} from 'node:util';

const run = promisify(execFile);
const epoch = Date.UTC(2001, 0, 1);
const DAY = 86_400_000;

export const messagesSendScript = `
on run argv
    set recipientAddress to item 1 of argv
    set messageText to item 2 of argv
    tell application "Messages"
        set accountToUse to first account whose service type is iMessage
        set recipientToUse to participant recipientAddress of accountToUse
        send messageText to recipientToUse
    end tell
end run
`;

// The app macOS asks about for Full Disk Access: the terminal that started the backend,
// or Chord itself when its launcher did.
export function accessApp(env = process.env) {
    const names = {Apple_Terminal: 'Terminal', 'iTerm.app': 'iTerm', vscode: 'Visual Studio Code', WarpTerminal: 'Warp', ghostty: 'Ghostty'};
    return env.TERM_PROGRAM ? names[env.TERM_PROGRAM] || env.TERM_PROGRAM : 'Chord';
}

// Most Messages rows written by recent macOS keep their text only in attributedBody, an
// NSAttributedString in Apple's typedstream format. The plain string follows the NSString
// class name and a '+' marker, prefixed by its length.
export function attributedText(blob) {
    if (!blob) return null;
    const bytes = Buffer.from(blob);
    let at = bytes.indexOf('NSString');
    if (at < 0) return null;
    at = bytes.indexOf(0x2b, at + 8);
    if (at < 0 || at + 1 >= bytes.length) return null;
    at += 1;
    let length = bytes[at];
    at += 1;
    if (length === 0x81) {
        if (at + 2 > bytes.length) return null;
        length = bytes.readUInt16LE(at);
        at += 2;
    } else if (length === 0x82) {
        if (at + 4 > bytes.length) return null;
        length = bytes.readUInt32LE(at);
        at += 4;
    }
    if (at + length > bytes.length) return null;
    return bytes.subarray(at, at + length).toString('utf8');
}

// Messages stores numbers as dialed or in E.164, Contacts as typed. Compare in E.164,
// assuming US numbers when there is no country code.
export function phoneKey(value) {
    const raw = String(value || '').trim();
    if (raw.includes('@')) return raw.toLowerCase();
    const digits = raw.replace(/\D/g, '');
    if (raw.startsWith('+')) return digits.length >= 7 ? '+' + digits : null;
    if (digits.length === 10) return '+1' + digits;
    if (digits.length === 11 && digits.startsWith('1')) return '+' + digits;
    return null;
}

function addressBooks(root) {
    const files = [path.join(root, 'AddressBook-v22.abcddb')];
    try {
        for (const source of readdirSync(path.join(root, 'Sources'))) {
            files.push(path.join(root, 'Sources', source, 'AddressBook-v22.abcddb'));
        }
    } catch {}
    return files;
}

// Contacts from every macOS address book (iCloud, Google, On My Mac), read-only.
export function readContacts(root = path.join(homedir(), 'Library/Application Support/AddressBook')) {
    const people = new Map();
    for (const file of addressBooks(root)) {
        let db;
        try {
            db = new DatabaseSync(file, {readOnly: true, timeout: 1500});
            for (const r of db.prepare('SELECT Z_PK AS id, ZFIRSTNAME AS first, ZLASTNAME AS last, ZNICKNAME AS nick, ZORGANIZATION AS org FROM ZABCDRECORD').all()) {
                const name = [r.first, r.last].filter(Boolean).join(' ').trim() || r.nick || r.org;
                if (name) people.set(file + ':' + r.id, {name, first: r.first || r.nick || r.last || r.org, phones: [], emails: []});
            }
            for (const r of db.prepare('SELECT ZOWNER AS owner, ZFULLNUMBER AS value FROM ZABCDPHONENUMBER').all()) {
                people.get(file + ':' + r.owner)?.phones.push(r.value);
            }
            for (const r of db.prepare('SELECT ZOWNER AS owner, ZADDRESS AS value FROM ZABCDEMAILADDRESS').all()) {
                people.get(file + ':' + r.owner)?.emails.push(r.value);
            }
        } catch {
            // A missing or locked address book only means fewer names.
        } finally {
            db?.close();
        }
    }
    return [...people.values()].filter(p => p.phones.length || p.emails.length);
}

let contactCache = {at: 0, root: null, people: [], index: new Map()};
function contactIndex(root) {
    if (Date.now() - contactCache.at < 60_000 && contactCache.root === root) return contactCache;
    const people = readContacts(root);
    const index = new Map();
    for (const person of people) {
        for (const value of [...person.phones, ...person.emails]) {
            const key = phoneKey(value);
            if (key && !index.has(key)) index.set(key, person);
        }
    }
    contactCache = {at: Date.now(), root, people, index};
    return contactCache;
}

// Unnamed groups read like Messages does: "Aiden, Ronav & 2 more".
export function groupName(names) {
    if (names.length <= 3) return names.join(', ');
    return `${names.slice(0, 2).join(', ')} & ${names.length - 2} more`;
}

export async function macMessages(method, args, {
    enabled = process.env.CHORD_MACOS_MESSAGES === '1',
    database = path.join(homedir(), 'Library/Messages/chat.db'),
    contacts = path.join(homedir(), 'Library/Application Support/AddressBook'),
    app = accessApp(),
    now = Date.now(),
    send = (address, text) => run('/usr/bin/osascript', ['-e', messagesSendScript, address, text], {timeout: 30_000}),
} = {}) {
    if (!enabled) {
        if (method === 'GetStatus') return {map: false, enabled: false, connectivity_detail: 'Turn on iMessages to read the Messages app on this Mac.'};
        if (method === 'ListThreads' || method === 'ListContacts') return [];
        throw new Error('iMessages is turned off');
    }
    if (method === 'ListContacts') {
        return contactIndex(contacts).people.map(({name, phones, emails}) => ({
            name,
            phones: phones.map(phoneKey).filter(key => key?.startsWith('+')),
            emails,
        }));
    }
    let db;
    try {
        db = new DatabaseSync(database, {readOnly: true, timeout: 1500});
        db.prepare('SELECT 1 FROM chat LIMIT 1').get();
    } catch {
        db?.close();
        const detail = `Chord can't read Messages yet. Give ${app} Full Disk Access, then restart Chord's backend.`;
        if (method === 'GetStatus') return {map: false, enabled: true, needs_access: true, access_app: app, connectivity_detail: detail};
        throw new Error(detail);
    }
    try {
        if (method === 'GetStatus') {
            return {map: true, enabled: true, connectivity_detail: 'Reading the Messages app on this Mac'};
        }
        const participants = db.prepare(`
            SELECT DISTINCT handle.id
            FROM chat_handle_join JOIN handle ON handle.ROWID = handle_id
            WHERE chat_id = ? ORDER BY handle.id
        `);
        function recipient(chat) {
            const handles = participants.all(chat.id);
            const address = handles[0]?.id;
            if (chat.style === 45 && chat.service === 'iMessage' &&
                handles.length === 1 && chat.address === address &&
                /^[+0-9][0-9]{2,}$|^[A-Za-z0-9_.+%-]+@[A-Za-z0-9.-]+$/.test(address)) return address;
        }
        if (method === 'SendToThreadChecked') {
            const [key, text, confirmedGroup] = args;
            const chat = db.prepare(`
                SELECT ROWID AS id, chat_identifier AS address, service_name AS service, style
                FROM chat WHERE ROWID = ?
            `).get(Number(String(key).match(/^mac-(\d+)$/)?.[1] || 0));
            const address = chat && recipient(chat);
            if (!address || confirmedGroup || typeof text !== 'string' || !text.trim() || text.includes('\0')) {
                throw new Error('Chord can send only to an existing one-to-one iMessage conversation.');
            }
            await send(address, text);
            return {submitted: true};
        }
        // The Messages database stays read-only. Opening a thread in Chord
        // does not mark it read in Apple Messages.
        if (method === 'MarkThreadRead') return false;
        if (method !== 'ListThreads') throw new Error('This action is unavailable for macOS Messages.');

        const {index} = contactIndex(contacts);
        const contact = handle => index.get(phoneKey(handle));
        const limit = Math.min(500, Math.max(1, Number(args[0]) || 200));
        // Tapbacks and other reactions are separate rows; show only real messages.
        const real = 'COALESCE(m.associated_message_type, 0) = 0';
        const chats = db.prepare(`
            SELECT c.ROWID AS id, c.chat_identifier AS address, c.service_name AS service,
                c.style, c.display_name AS name, max(m.date) / 1000000 AS latest,
                max(CASE WHEN m.is_from_me = 1 THEN m.date END) / 1000000 AS sent,
                sum(CASE WHEN m.is_from_me = 0 AND COALESCE(m.is_read, 1) = 0 AND m.date / 1000000 > ? THEN 1 ELSE 0 END) AS unread,
                max(CASE WHEN m.is_from_me = 0 AND COALESCE(m.is_read, 1) = 0 THEN m.date END) / 1000000 AS unread_latest
            FROM chat c JOIN chat_message_join cm ON cm.chat_id = c.ROWID
            JOIN message m ON m.ROWID = cm.message_id
            WHERE c.service_name IN ('iMessage', 'SMS', 'RCS') AND ${real}
            GROUP BY c.ROWID ORDER BY latest DESC, c.ROWID DESC LIMIT ?
        `).all(now - 30 * DAY - epoch, limit);
        const messages = db.prepare(`
            SELECT m.ROWID AS id, m.text, m.attributedBody AS rich, m.date / 1000000 AS date, m.is_from_me, h.id AS sender
            FROM chat_message_join cm JOIN message m ON m.ROWID = cm.message_id
            LEFT JOIN handle h ON h.ROWID = m.handle_id
            WHERE cm.chat_id = ? AND ${real} AND ((m.text IS NOT NULL AND m.text <> '') OR m.attributedBody IS NOT NULL)
            ORDER BY m.date DESC, m.ROWID DESC LIMIT 50
        `);
        const timestamp = value => new Date(epoch + value).toISOString();
        return chats.map(chat => {
            const handles = participants.all(chat.id).map(handle => handle.id);
            const isGroup = chat.style !== 45 || handles.length !== 1;
            const people = handles.map(handle => contact(handle));
            const name = chat.name || (isGroup
                // Known people first; unknown numbers fold into "& N more".
                ? groupName([...people.filter(Boolean).map(person => person.first), ...handles.filter((_, i) => !people[i])])
                : people[0]?.name) || handles.join(', ') || chat.address;
            return {
                key: 'mac-' + chat.id,
                name,
                service: chat.service,
                last_ts: timestamp(chat.latest),
                last_sent_ts: chat.sent ? timestamp(chat.sent) : null,
                recipients: handles,
                participant_names: handles.map((handle, i) => people[i]?.name || handle),
                is_group: isGroup,
                reply_ready: !!recipient(chat),
                unread: chat.unread > 0,
                unread_count: chat.unread,
                last_unread_ts: chat.unread > 0 && chat.unread_latest ? timestamp(chat.unread_latest) : null,
                messages: messages.all(chat.id).reverse().flatMap(message => {
                    // U+FFFC marks an attachment's place in the text.
                    const body = (message.text || attributedText(message.rich) || '').replace(/￼/g, '').trim();
                    if (!body) return [];
                    return [{
                        handle: String(message.id),
                        body,
                        timestamp: timestamp(message.date),
                        outgoing: !!message.is_from_me,
                        sender: message.is_from_me ? 'You' : contact(message.sender)?.name || message.sender,
                        address: message.sender,
                    }];
                }),
            };
        });
    } finally {
        db.close();
    }
}
