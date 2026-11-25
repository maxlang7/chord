import test from 'node:test';
import assert from 'node:assert/strict';
import {ContactBook, phoneUnread} from '../contacts.mjs';

const memoryBook = () => {const book = new ContactBook('/unused'); book.save = async () => {}; return book;};

test('reading a phone conversation in Chord is remembered and survives other rules', async () => {
    const book = memoryBook();
    assert.equal(book.preference('blueferry:mac-1').readAt, 0);
    await book.markRead('blueferry:mac-1', 1_000);
    assert.equal(book.preference('blueferry:mac-1').readAt, 1_000);
    // Muting or marking Important rewrites the entry; it must keep the read time.
    await book.setPreference('blueferry:mac-1', {priority: 'important'});
    await book.setPreference('blueferry:mac-1', {muteMinutes: 60});
    assert.equal(book.preference('blueferry:mac-1').readAt, 1_000);
    assert.equal(book.preference('blueferry:mac-1').priority, 'important');
});

test('a phone conversation stays read until a message arrives after Chord opened it', () => {
    const at = iso => Date.parse(iso);
    const thread = {unread: true, last_ts: '2026-11-22T10:05:00Z', last_unread_ts: '2026-11-22T10:00:00Z'};
    assert.equal(phoneUnread(thread, 0), true);
    assert.equal(phoneUnread(thread, at('2026-11-22T10:01:00Z')), false);
    assert.equal(phoneUnread(thread, at('2026-11-22T09:59:00Z')), true);
    assert.equal(phoneUnread({...thread, unread: false}, 0), false);
    // Without a per-message unread time (BlueFerry), fall back to the newest message.
    assert.equal(phoneUnread({unread: true, last_ts: '2026-11-22T10:05:00Z'}, at('2026-11-22T10:01:00Z')), true);
    assert.equal(phoneUnread({unread: true, last_ts: '2026-11-22T10:05:00Z'}, at('2026-11-22T10:06:00Z')), false);
});
