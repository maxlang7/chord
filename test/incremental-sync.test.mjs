import test from 'node:test';
import assert from 'node:assert/strict';
import {MatrixProvider} from '../matrix-provider.mjs';

test('incremental room sync keeps titles and accounts and still updates unreads',async()=>{
 const p=new MatrixProvider({},{userId:'@me'},{save:false});
 p.chats.set('!quiet',{id:'!quiet',accountID:'slack',network:'Slack',title:'Quiet',type:'group',unreadCount:4,participants:{items:[{id:'@a:chord.local',fullName:'A',isSelf:false}],total:3,hasMore:true}});
 const since=[];
 let phase='full';
 p.raw=async(_m,_p,_b,query)=>{
  since.push(query.since||null);
  assert.equal(JSON.parse(query.filter).room.timeline.unread_thread_notifications,true);
  assert.equal(query.timeout,'0');
  if(phase==='full')return {next_batch:'s1',account_data:{events:[{type:'m.direct',content:{'@x:chord.local':['!dm']}}]},rooms:{join:{
   '!slack':{state:{events:[{type:'m.room.name',content:{name:'Lab'}},{type:'m.bridge',content:{protocol:{id:'slack',displayname:'Slack'},channel:{displayname:'Lab'}},},{type:'m.room.member',state_key:'@me',content:{membership:'join',displayname:'Me'}},{type:'m.room.member',state_key:'@a:chord.local',content:{membership:'join',displayname:'Ada'}}]},unread_notifications:{notification_count:2,highlight_count:1}},
   '!dm':{state:{events:[{type:'m.room.member',state_key:'@x:chord.local',content:{membership:'join',displayname:'Xin'}}]},summary:{'m.joined_member_count':2},unread_notifications:{notification_count:0,highlight_count:0}},
  }}};
  return {next_batch:'s2',rooms:{join:{
   '!slack':{timeline:{events:[{type:'m.room.message',origin_server_ts:5000,content:{body:'hello'}}]},unread_notifications:{notification_count:5,highlight_count:2}},
   '!new':{state:{events:[{type:'m.room.name',content:{name:'New room'}},{type:'m.bridge',content:{protocol:{id:'discordgo',displayname:'Discord'}}}]},unread_notifications:{notification_count:1,highlight_count:0}},
  }}};
 };
 await p.refreshRoomsNow();
 assert.equal(p.chats.get('!slack').title,'Lab');
 assert.equal(p.chats.get('!slack').accountID,'slack');
 assert.equal(p.chats.get('!dm').type,'single');
 assert.equal(p.chats.get('!dm').title,'Xin');
 assert.equal(p.chats.get('!quiet').unreadCount,4);
 phase='incremental';
 await p.refreshRoomsNow();
 assert.deepEqual(since,[null,'s1']);
 const slack=p.chats.get('!slack');
 assert.equal(slack.title,'Lab');
 assert.equal(slack.accountID,'slack');
 assert.equal(slack.network,'Slack');
 assert.equal(slack.type,'group');
 assert.equal(slack.unreadCount,5);
 assert.equal(slack.highlightCount,2);
 assert.equal(slack.preview.text,'hello');
 assert.equal(slack.participants.items.length,2);
 assert.equal(p.chats.get('!dm').type,'single');
 assert.equal(p.chats.get('!dm').title,'Xin');
 assert.equal(p.chats.get('!quiet').title,'Quiet');
 assert.equal(p.chats.get('!quiet').accountID,'slack');
 assert.equal(p.chats.get('!quiet').unreadCount,4);
 assert.equal(p.chats.get('!new').title,'New room');
 assert.equal(p.chats.get('!new').accountID,'discord');
 assert.equal(p.syncToken,'s2');
});

test('a rejected sync token falls back to a full sync',async()=>{
 const p=new MatrixProvider({},{userId:'@me'},{save:false});
 p.syncToken='stale';
 p.chats.set('!slack',{id:'!slack',accountID:'slack',network:'Slack',title:'Lab',type:'group'});
 const since=[];
 p.raw=async(_m,_p,_b,query)=>{
  since.push(query.since||null);
  if(query.since)throw Object.assign(new Error('Unknown pos'),{status:400});
  return {next_batch:'s2',rooms:{join:{'!slack':{state:{events:[{type:'m.room.name',content:{name:'Lab renamed'}},{type:'m.bridge',content:{protocol:{id:'slack',displayname:'Slack'}}}]},unread_notifications:{notification_count:0,highlight_count:0}}}}};
 };
 await p.refreshRoomsNow();
 assert.deepEqual(since,['stale',null]);
 assert.equal(p.chats.get('!slack').title,'Lab renamed');
 assert.equal(p.chats.get('!slack').accountID,'slack');
 assert.equal(p.syncToken,'s2');
});

test('a homeserver error keeps the sync token',async()=>{
 const p=new MatrixProvider({},{userId:'@me'},{save:false});
 p.syncToken='keep';
 p.raw=async()=>{throw Object.assign(new Error('down'),{status:500})};
 await assert.rejects(()=>p.refreshRoomsNow(),/down/);
 assert.equal(p.syncToken,'keep');
});
