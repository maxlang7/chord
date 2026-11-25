import assert from 'node:assert/strict';
import {mkdir, mkdtemp} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import test from 'node:test';
import {accessApp, attributedText, groupName, groupToken, macMessages, phoneKey} from '../macos-messages.mjs';

// A minimal typedstream NSAttributedString, as Messages writes to attributedBody.
function richBody(text) {
    const bytes = Buffer.from(text, 'utf8');
    const length = bytes.length < 0x80 ? Buffer.from([bytes.length]) : Buffer.from([0x81, bytes.length & 0xff, bytes.length >> 8]);
    return Buffer.concat([Buffer.from('\x04\x0bstreamtyped\x81\xe8\x03\x84\x01@\x84\x84\x84\x12NSAttributedString\x00\x84\x84\x08NSObject\x00\x85\x92\x84\x84\x84\x08NSString\x01\x94\x84\x01+', 'latin1'), length, bytes, Buffer.from('\x86\x84\x02iI', 'latin1')]);
}

async function fixture() {
    const directory = await mkdtemp(path.join(tmpdir(), 'chord-messages-'));
    const database = path.join(directory, 'chat.db');
    const db = new DatabaseSync(database);
    db.exec(`
        CREATE TABLE chat (guid TEXT, chat_identifier TEXT, service_name TEXT, style INTEGER, display_name TEXT);
        CREATE TABLE chat_handle_join (chat_id INTEGER, handle_id INTEGER);
        CREATE TABLE handle (id TEXT);
        CREATE TABLE message (text TEXT, attributedBody BLOB, date INTEGER, is_from_me INTEGER, is_read INTEGER, handle_id INTEGER, associated_message_type INTEGER);
        CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);
        INSERT INTO chat VALUES
            ('any;-;dad@example.org', 'dad@example.org', 'iMessage', 45, NULL),
            ('any;+;chat100', 'chat100', 'iMessage', 43, 'Family'),
            ('any;-;+15550123456', '+15550123456', 'SMS', 45, NULL),
            ('any;+;chat101', 'chat101', 'iMessage', 43, NULL),
            ('any;+;chat200', 'chat200', 'SMS', 43, NULL);
        INSERT INTO handle VALUES ('dad@example.org'), ('+15550123456'), ('+15550000001'), ('+15550000002');
        INSERT INTO chat_handle_join VALUES (1, 1), (2, 1), (2, 2), (3, 2), (4, 1), (4, 2), (4, 3), (4, 4), (5, 1), (5, 2);
        INSERT INTO chat_message_join VALUES (1, 1), (1, 2), (2, 1), (3, 3), (1, 4), (4, 5);
    `);
    const insert = db.prepare('INSERT INTO message VALUES (?, ?, ?, ?, ?, ?, ?)');
    insert.run('hello', null, 800000000000000000, 0, 1, 1, 0);
    insert.run('hi', null, 800000001000000000, 1, 1, null, 0);
    insert.run(null, richBody('see you at 6 ￼'), 800000002000000000, 0, 0, 2, 0);
    insert.run('Loved “hello”', null, 800000003000000000, 1, 1, null, 2000);
    insert.run(null, richBody('x'.repeat(200)), 799000000000000000, 1, 1, null, 0);
    db.close();

    const contacts = path.join(directory, 'AddressBook');
    await mkdir(path.join(contacts, 'Sources', 'icloud'), {recursive: true});
    const book = new DatabaseSync(path.join(contacts, 'Sources', 'icloud', 'AddressBook-v22.abcddb'));
    book.exec(`
        CREATE TABLE ZABCDRECORD (Z_PK INTEGER, ZFIRSTNAME TEXT, ZLASTNAME TEXT, ZNICKNAME TEXT, ZORGANIZATION TEXT);
        CREATE TABLE ZABCDPHONENUMBER (ZOWNER INTEGER, ZFULLNUMBER TEXT);
        CREATE TABLE ZABCDEMAILADDRESS (ZOWNER INTEGER, ZADDRESS TEXT);
        INSERT INTO ZABCDRECORD VALUES (1, 'Sam', 'Lee', NULL, NULL), (2, NULL, NULL, NULL, 'Dad Co'), (3, 'Ana', NULL, NULL, NULL);
        INSERT INTO ZABCDPHONENUMBER VALUES (1, '(555) 012-3456'), (3, '+1 555 000 0001');
        INSERT INTO ZABCDEMAILADDRESS VALUES (2, 'Dad@Example.org');
    `);
    book.close();
    return {database, contacts};
}

test('reads Messages in order and only sends to a verified direct iMessage chat', async () => {
    const {database, contacts} = await fixture();
    const sent = [];
    const now = Date.UTC(2026, 4, 9, 12);
    const options = {enabled: true, database, contacts, now, send: (...args) => sent.push(args)};
    const threads = await macMessages('ListThreads', [50], options);
    const byKey = Object.fromEntries(threads.map(t => [t.key, t]));
    assert.deepEqual(threads.map(t => t.key), ['mac-3', 'mac-1', 'mac-2', 'mac-4']);

    const direct = byKey['mac-1'];
    assert.equal(direct.reply_ready, true);
    assert.equal(direct.name, 'Dad Co');
    assert.deepEqual(direct.messages.map(message => message.body), ['hello', 'hi']);
    assert.equal(direct.messages[0].sender, 'Dad Co');
    assert.equal(direct.messages[1].timestamp, '2026-05-09T06:13:21.000Z');
    assert.equal(direct.last_sent_ts, '2026-05-09T06:13:21.000Z');

    const sms = byKey['mac-3'];
    assert.equal(sms.service, 'SMS');
    assert.equal(sms.name, 'Sam Lee');
    assert.equal(sms.reply_ready, false);
    assert.equal(sms.messages[0].body, 'see you at 6');
    assert.equal(sms.unread, true);
    assert.equal(sms.unread_count, 1);
    assert.equal(sms.last_unread_ts, '2026-05-09T06:13:22.000Z');
    assert.equal(direct.last_unread_ts, null);

    assert.equal(byKey['mac-2'].name, 'Family');
    assert.equal(byKey['mac-2'].reply_ready, true);
    assert.equal(byKey['mac-4'].is_group, true);
    assert.equal(byKey['mac-4'].name, 'Ana, Sam & 2 more');
    assert.equal(byKey['mac-4'].messages[0].body, 'x'.repeat(200));

    await macMessages('SendToThreadChecked', ['mac-1', 'a "quoted" message', false], options);
    assert.deepEqual(sent, [['dad@example.org', 'a "quoted" message']]);

    // Groups need the confirmation flag and a roster that still matches what Chord showed.
    const groupSent = [];
    const group = {...options, sendGroup: (...args) => groupSent.push(args)};
    const token = groupToken(byKey['mac-2'].recipients);
    assert.equal(token, '\n+15550123456\ndad@example.org');
    await assert.rejects(macMessages('SendToThreadChecked', ['mac-2', 'group', false, token], group), /not SMS or RCS/);
    await assert.rejects(macMessages('SendToThreadChecked', ['mac-2', 'group', true, groupToken(['+15550123456'])], group), /group changed/);
    await macMessages('SendToThreadChecked', ['mac-2', 'hi all', true, token], group);
    assert.deepEqual(groupSent, [['any;+;chat100', 'hi all']]);
    await assert.rejects(macMessages('SendToThreadChecked', ['mac-5', 'sms group', true, groupToken(['+15550123456', 'dad@example.org'])], group), /not SMS or RCS/);
    await assert.rejects(macMessages('SendToThreadChecked', ['mac-3', 'SMS', false], options));
    await assert.rejects(macMessages('SendToThreadChecked', ['mac-2', '   ', true, token], group));
    assert.equal(groupSent.length, 1);
    assert.equal(sent.length, 1);
    assert.deepEqual(await macMessages('ListThreads', [10], {enabled: false, database}), []);
});

test('contacts, status and the off switch', async () => {
    const {database, contacts} = await fixture();
    const people = await macMessages('ListContacts', [], {enabled: true, database, contacts});
    assert.deepEqual(people.find(p => p.name === 'Sam Lee').phones, ['+15550123456']);
    assert.deepEqual(await macMessages('GetStatus', [], {enabled: false}), {map: false, enabled: false, connectivity_detail: 'Turn on iMessages to read the Messages app on this Mac.'});
    const blocked = await macMessages('GetStatus', [], {enabled: true, database: path.join(tmpdir(), 'missing', 'chat.db'), app: 'Terminal'});
    assert.equal(blocked.needs_access, true);
    assert.equal(blocked.access_app, 'Terminal');
    assert.match(blocked.connectivity_detail, /Give Terminal Full Disk Access/);
    assert.equal((await macMessages('GetStatus', [], {enabled: true, database})).map, true);
});

test('helpers', () => {
    assert.equal(phoneKey('(908) 902-3769'), '+19089023769');
    assert.equal(phoneKey('1-908-902-3769'), '+19089023769');
    assert.equal(phoneKey('+44 20 7946 0958'), '+442079460958');
    assert.equal(phoneKey('Dad@Example.org'), 'dad@example.org');
    assert.equal(phoneKey('12345'), null);
    assert.equal(groupName(['A', 'B', 'C']), 'A, B, C');
    assert.equal(groupName(['A', 'B', 'C', 'D', 'E']), 'A, B & 3 more');
    assert.equal(attributedText(null), null);
    for (const marker of [0x81, 0x82]) {
        assert.equal(attributedText(Buffer.concat([Buffer.from('NSString+'), Buffer.from([marker])])), null);
    }
    assert.equal(attributedText(richBody('héllo')), 'héllo');
    assert.equal(accessApp({TERM_PROGRAM: 'Apple_Terminal'}), 'Terminal');
    assert.equal(accessApp({}), 'Chord');
});
