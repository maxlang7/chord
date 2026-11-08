"""Read Beeper's cache without modifying it. Write aggregate data privately."""
import collections
import datetime
import json
import os
from pathlib import Path
import sqlite3

ROOT = Path(__file__).resolve().parents[1]
DB = Path.home() / '.config/BeeperTexts/index.db'
db = sqlite3.connect(f'file:{DB}?mode=ro', uri=True)
threads = {r[0]: (r[1], json.loads(r[2])) for r in db.execute('select threadID,accountID,thread from threads')}
networks = collections.defaultdict(lambda: collections.Counter(chats=0, messages=0, sent=0, groups=0, unread=0, muted=0, replies=0, attachments=0))
names = {'discordgo': 'Discord', 'matrix': 'Beeper / Matrix', 'hungryserv': 'Beeper / Matrix'}
def network(account):
    if 'blueferry' in account:
        return 'iPhone / BlueFerry'
    if account.startswith('slack'):
        return 'Slack'
    if account.startswith('whatsapp'):
        return 'WhatsApp'
    return names.get(account, account)

for room, (account, thread) in threads.items():
    if account.startswith('$') or (account == 'hungryserv' and not thread.get('timestamp')):
        continue
    stats = networks[network(account)]
    stats['chats'] += 1
    stats['groups'] += thread.get('type') == 'group'
    stats['unread'] += bool(thread.get('unreadCount'))
    stats['muted'] += bool(thread.get('isMuted') or thread.get('extra', {}).get('muted'))

mine_replies = mine_attachments = total_attachments = 0
cutoff = int((datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(days=30)).timestamp()*1000)
recent = collections.Counter()
for room, sent, reply, stamp, raw in db.execute('select roomID,isSentByMe,inReplyToID,timestamp,message from mx_room_messages where isDeleted=0'):
    account = threads.get(room, ('Unknown', {}))[0]
    stats = networks[network(account)]
    stats['messages'] += 1
    stats['sent'] += bool(sent)
    stats['replies'] += bool(reply)
    content = json.loads(raw)
    has_attachment = bool(content.get('attachments'))
    stats['attachments'] += has_attachment
    total_attachments += has_attachment
    mine_attachments += has_attachment and bool(sent)
    mine_replies += bool(reply) and bool(sent)
    if stamp >= cutoff:
        recent[network(account)] += 1

my_reactions, total_reactions = db.execute('select coalesce(sum(isSentByMe),0),count(*) from mx_reactions where isDeleted=0').fetchone()
result = {
    'auditedAt': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'source': 'Read-only SQLite cache metadata, not screen time or a complete network archive',
    'networks': [dict(network=name, **counts, recent30Days=recent[name]) for name, counts in sorted(networks.items())],
    'totalMessages': sum(n['messages'] for n in networks.values()),
    'myReplies': mine_replies, 'myReactions': my_reactions,
    'myAttachments': mine_attachments, 'totalAttachments': total_attachments,
    'totalReactions': total_reactions,
    'uses': ['Slack teaching coordination and small group discussions', 'WhatsApp tutoring coordination, friends and family', 'Discord community channels, clubs and direct messages', 'iPhone messages through BlueFerry'],
    'limits': ['Usage categories inferred from chat titles and a bounded sample of live conversations', 'Muted and read counters depend on fields available in the cache', 'No screen-time or click telemetry was inspected'],
}
out=ROOT/'private/audit.json'
out.parent.mkdir(mode=0o700, exist_ok=True)
out.write_text(json.dumps(result, indent=2)+'\n')
os.chmod(out,0o600)
print(json.dumps(result,indent=2))
