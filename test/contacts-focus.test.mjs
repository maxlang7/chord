import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {ContactBook} from '../contacts.mjs';
import {Directory} from '../directory.mjs';
import {importance,rankInbox} from '../importance.mjs';

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
 await book.setPreference('chat:one',{priority:'auto'});
 assert.equal(rankInbox(conversations,book,{view:'focus',now}).items.length,2);
});
