import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {ContactBook} from '../contacts.mjs';
import {Directory} from '../directory.mjs';
import {applyLocalRules,importance,rankInbox} from '../importance.mjs';

const memoryBook=()=>{const book=new ContactBook('/unused');book.save=async()=>{};return book};

test('matching names do not link people, exact verified phone identifiers do',()=>{
 const book=memoryBook();
 const alice=book.observe({key:'@discord_1:chord.local',name:'Alex',source:'discord'});
 const another=book.observe({key:'@discord_2:chord.local',name:'Alex',source:'discord'});
 assert.notEqual(alice.id,another.id);
 book.observe({key:'tel:+14155550100',name:'Alex Chen',source:'iphone',rank:100});
 book.observe({key:'@whatsapp_14155550100:chord.local',aliases:['tel:+14155550100'],name:'Alex',source:'whatsapp'});
 assert.equal(book.person('tel:+14155550100').id,book.person('@whatsapp_14155550100:chord.local').id);
 assert.equal(book.name('@whatsapp_14155550100:chord.local'),'Alex Chen');
 assert.equal(book.name('@discord_2:chord.local'),'Alex');
});

test('names and favorites persist and survive later network-profile updates',async()=>{
 const dir=await mkdtemp(path.join(os.tmpdir(),'chord-contact-test-'));
 const book=await new ContactBook(dir).load();
 const person=book.observe({key:'@slack_t1-u1:chord.local',name:'Alex',source:'slack'});
 await book.update(person.id,{name:'Alex from lab',favorite:true});
 const loaded=await new ContactBook(dir).load();
 loaded.observe({key:'@slack_t1-u1:chord.local',name:'Alexander',source:'slack',rank:60});
 assert.equal(loaded.name('@slack_t1-u1:chord.local'),'Alex from lab');
 assert.equal(loaded.person('@slack_t1-u1:chord.local').favorite,true);
});

test('explicit contact links can be undone without losing either record',async()=>{
 const book=memoryBook();
 const a=book.observe({key:'@slack_t1-u1:chord.local',name:'Alex',source:'slack'});
 const b=book.observe({key:'@discord_1:chord.local',name:'Alex',source:'discord'});
 await book.update(a.id,{name:'Alex Chen',favorite:true});
 await book.merge(a.id,b.id);
 assert.equal(book.person('@slack_t1-u1:chord.local').id,b.id);
 await book.undoMerge();
 assert.equal(book.person('@slack_t1-u1:chord.local').id,a.id);
 assert.equal(book.name('@slack_t1-u1:chord.local'),'Alex Chen');
 assert.equal(book.person('@discord_1:chord.local').id,b.id);
});

test('message senders use contact names and local renames across linked accounts',async()=>{
 const book=memoryBook();const person=book.observe({key:'@slack_t1-u1:chord.local',name:'Alex',source:'slack'});
 await book.update(person.id,{name:'Alex from lab'});
 const directory=new Directory(book,{blue:async()=>[],config:()=>({})});
 const [m]=await directory.enrichMessages([{senderID:'@slack_t1-u1:chord.local',senderName:'@slack_t1-u1:chord.local',text:'Hello'}],{source:'slack'});
 assert.equal(m.senderName,'Alex from lab');assert.equal(m.senderContactID,person.id);
});

test('Focus keeps personal conversations and explicit signals, hiding unengaged channels',()=>{
 const now=Date.parse('2026-10-29T12:00:00Z');const timestamp=new Date(now-3600000).toISOString();
 const base={timestamp,source:'discord',kind:'chat'};
 assert.equal(importance({...base,direct:true,unread:true},{now}).important,true);
 assert.equal(importance({...base,direct:false,unread:true},{now}).important,false);
 assert.equal(importance({...base,signals:{mention:true}},{now}).important,true);
 assert.equal(importance({...base,signals:{replyToYou:true}},{now}).important,true);
 assert.equal(importance({...base,activity:{lastSentAt:now-86400000}},{now}).important,true);
 assert.equal(importance({...base,isBot:true},{now}).important,false);
 assert.equal(importance({...base,direct:true,isMuted:true},{now}).important,false);
 assert.equal(importance({...base,kind:'email',unread:true,labels:['CATEGORY_PROMOTIONS']},{now}).important,false);
 assert.equal(importance({...base,kind:'email',unread:true,signals:{automated:true}},{now}).important,false);
});

test('quiet and pin overrides persist, All still contains every conversation',async()=>{
 const book=memoryBook();const now=Date.now(),timestamp=new Date(now).toISOString();
 const conversations=[{id:'chat:one',timestamp,direct:true},{id:'chat:two',timestamp,direct:false}];
 await book.setPreference('chat:one',{priority:'quiet'});
 await book.setPreference('chat:two',{priority:'important'});
 const focus=rankInbox(conversations,book,{view:'focus',now});
 assert.deepEqual(focus.items.map(c=>c.id),['chat:two']);assert.equal(focus.summary.hidden,1);
 assert.equal(rankInbox(conversations,book,{view:'all',now}).items.length,2);
 const ruled=applyLocalRules([{id:'chat:one',timestamp,unread:true,unreadCount:4,highlightCount:2,attention:{important:true,reason:'All messages in a selected channel'}}],book,now);
 assert.equal(ruled[0].unread,false);
 assert.equal(ruled[0].unreadCount,0);
 assert.equal(ruled[0].highlightCount,0);
 assert.equal(ruled[0].sourceUnread,true);
 assert.equal(rankInbox(ruled,book,{view:'focus',now}).items.length,0);
 await book.setPreference('chat:one',{priority:'auto'});
 assert.equal(rankInbox(conversations,book,{view:'focus',now}).items.length,2);
});
test('a mute lasts for one of five durations, hides unread, and keeps the priority underneath',async()=>{
 const book=memoryBook();
 const now=Date.parse('2026-11-12T15:00:00Z');
 const chat={id:'chat:one',timestamp:new Date(now).toISOString(),direct:true,unread:true};
 await assert.rejects(()=>book.setPreference('chat:one',{muteMinutes:90},now),/1 hour, 4 hours, 24 hours, 3 days, or 7 days/);
 for(const minutes of [60,240,1440,4320,10080]){
  const saved=await book.setPreference('chat:one',{priority:'important',muteMinutes:minutes},now);
  assert.equal(saved.priority,'important');
  assert.equal(saved.muted,true);
  assert.equal(saved.muteUntil,now+minutes*60000);
 }
 const hidden=applyLocalRules([chat],book,now);
 assert.equal(hidden[0].unread,false);
 assert.equal(hidden[0].sourceUnread,true);
 assert.equal(hidden[0].localPriority,'important');
 assert.equal(rankInbox(hidden,book,{view:'focus',now}).items.length,0);
 const later=now+10080*60000+1;
 assert.equal(book.preference('chat:one',later).muted,false);
 const back=applyLocalRules([{...chat,unread:true}],book,later);
 assert.equal(back[0].unread,true);
 const stuck=applyLocalRules([{...chat,unread:true,isMuted:true,muteUntil:now}],book,later);
 assert.equal(stuck[0].isMuted,false);
 assert.equal(stuck[0].unread,true);
 assert.equal(rankInbox(back,book,{view:'focus',now:later}).items.map(c=>c.id).includes('chat:one'),true);
 await book.setPreference('chat:one',{muteMinutes:60},now);
 await book.setPreference('chat:one',{muteMinutes:null},now);
 assert.equal(book.preference('chat:one',now).muted,false);
 assert.equal(book.preference('chat:one',now).priority,'important');
});
test('direct conversations choose the other person for photos, excluding bridge bots and self',()=>{
 const book=memoryBook();
 const bot=book.observe({key:'@slackbot:chord.local',name:'Chord Slack'});
 const jiao=book.observe({key:'@slack_team-jiao:chord.local',name:'Jiao Deng',source:'slack'});
 const me=book.observe({key:'@max:chord.local',name:'Max'});
 const other=book.observe({key:'@slack_team-other:chord.local',name:'Other participant',source:'slack'});
 const directory=new Directory(book,{blue:async()=>[],config:()=>({})});
 directory.catalog.selfIds=new Set(['@max:chord.local']);
 directory.catalog.rooms.set('!dm',{network:'slack',direct:true,otherUser:'@slack_team-jiao:chord.local'});
 const conversation=directory.enrichConversation({id:'matrix:!dm',remoteId:'!dm',kind:'chat',source:'slack',participants:['@slackbot:chord.local','@max:chord.local','@slack_team-other:chord.local','@slack_team-jiao:chord.local']});
 assert.equal(conversation.title,'Jiao Deng');assert.equal(conversation.contactIDs[0],jiao.id);
 assert.equal(conversation.contactIDs.includes(bot.id),false);assert.equal(conversation.contactIDs.includes(me.id),false);assert.equal(conversation.contactIDs.includes(other.id),true);
});
test('every inbox view sorts by recency, even when an older item has a higher importance score',()=>{
 const book=memoryBook();book.data.conversations.old={priority:'important'};
 const items=[{id:'old',timestamp:'2026-10-01T00:00:00Z'},{id:'new',timestamp:'2026-11-09T00:00:00Z',attention:{important:true,reason:'Selected channel'}},{id:'invalid',timestamp:'invalid',attention:{important:true,reason:'Selected channel'}}];
 for(const view of ['all','focus'])assert.deepEqual(rankInbox(items,book,{view}).items.map(c=>c.id),['new','old','invalid']);
});
test('older bridge activity cannot push a newly active conversation back into October',()=>{
 const book=memoryBook(),directory=new Directory(book,{blue:async()=>[],config:()=>({})});
 directory.catalog.rooms.set('!room',{activity:{lastMessageAt:Date.parse('2026-10-01T00:00:00Z')}});
 const c=directory.enrichConversation({id:'matrix:!room',remoteId:'!room',participants:[],timestamp:'2026-11-09T00:00:00Z'});
 assert.equal(c.timestamp,'2026-11-09T00:00:00.000Z');
});
test('group photos use at most three distinct humans and exclude phone conversations',()=>{
 const book=memoryBook(),directory=new Directory(book,{blue:async()=>[],config:()=>({})});
 const keys=['me','bot','one','two','three','four'].map(s=>'@slack_'+s+':chord.local');
 keys.forEach((key,i)=>book.observe({key,name:['Me','Bridge','One','Two','Three','Four'][i],source:'slack',isBot:i===1}));
 directory.catalog.selfIds=new Set([keys[0]]);
 directory.catalog.rooms.set('!group',{direct:false,avatarMembers:keys});
 for(const source of ['slack','discord','whatsapp']){
  const group=directory.enrichConversation({id:'matrix:!group',remoteId:'!group',source,participants:[keys[2],keys[2]]});
  assert.deepEqual(group.avatarContacts.map(p=>p.name),['One','Two','Three']);
 }
 const phone=directory.enrichConversation({id:'phone',kind:'phone',source:'iphone',participants:keys,blue:{is_group:true}});
 assert.deepEqual(phone.avatarContacts,[]);
});
