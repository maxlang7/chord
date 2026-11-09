import test from 'node:test';
import assert from 'node:assert/strict';
import {MatrixProvider} from '../matrix-provider.mjs';
test('mark read advances unopened and partially cached rooms to the current head',async()=>{
 for(const cached of [[],[{id:'$old'}]]){
  const receipts=[];const p=new MatrixProvider({}, {userId:'@me'},{save:false});
  p.chats.set('!room',{id:'!room',unreadCount:5});p.timelines.set('!room',cached);
  p.raw=async(method,url,body,query)=>{assert.equal(query.dir,'b');assert.equal(query.limit,'1');return {chunk:[{event_id:'$head'}]}};
  p.client={accountData:{setRoom:async()=>{}},messages:{markRead:async r=>receipts.push(r)}};
  await p.request('/v1/chats/!room/read','POST',{});
  assert.deepEqual(receipts,[{roomId:'!room',eventId:'$head'}]);assert.equal(p.chats.get('!room').unreadCount,0);
 }
});
test('a read watermark survives stale server counts and permits future unread messages',async()=>{
 const p=new MatrixProvider({}, {userId:'@me'},{save:false});
 p.chats.set('!room',{id:'!room',localReadAt:2000});
 let timestamp=1000;
 p.raw=async()=>({rooms:{join:{'!room':{timeline:{events:[{event_id:'$head',origin_server_ts:timestamp}]},unread_notifications:{notification_count:3,highlight_count:1}}}}});
 await p.refreshRoomsNow();assert.equal(p.chats.get('!room').unreadCount,0);
 timestamp=3000;
 await p.refreshRoomsNow();assert.equal(p.chats.get('!room').unreadCount,3);
});
