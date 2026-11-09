import test from 'node:test';import assert from 'node:assert/strict';import {MatrixProvider} from '../matrix-provider.mjs';
const event=(id,root)=>({eventId:id,roomId:'!room',text:id,timestamp:id==='$root'?1000:2000,sender:{userId:'@other'},threadRoot:root});
test('thread pages include the original parent and preserve cursors and thread identity',async()=>{
 const calls=[];const p=new MatrixProvider({messages:{list:async options=>{calls.push(options);return {messages:[event('$reply','$root')],nextCursor:'older'}},get:async()=>({message:event('$root')})}},{userId:'@me'},{save:false});
 p.chats.set('!room',{accountID:'slack',participants:{items:[]}});p.refreshReactions=async()=>{};
 const page=await p.request('/v1/chats/!room/messages?threadRoot=%24root&cursor=cursor');
 assert.equal(calls[0].threadRoot,'$root');assert.equal(calls[0].cursor,'cursor');assert.equal(page.oldestCursor,'older');assert.equal(page.hasMore,true);
 assert.deepEqual(page.items.map(m=>m.id),['$root','$reply']);assert.equal(page.items[1].threadRootID,'$root');assert.equal(page.items[0].replyCount,1);
 const channel=await p.request('/v1/chats/!room/messages');assert.equal(calls[1].threadRoot,undefined);assert.equal(channel.items[0].id,'$root');
});
test('Slack replies target the original thread root, including replies to a reply and media',async()=>{
 const sent=[],media=[];const p=new MatrixProvider({messages:{send:async o=>{sent.push(o);return {eventId:'$sent'}},sendMedia:async o=>{media.push(o);return {eventId:'$media'}}}},{userId:'@me'},{save:false});
 p.chats.set('!room',{accountID:'slack'});p.timelines.set('!room',[{id:'$root'},{id:'$reply',threadRootID:'$root'}]);
 await p.request('/v1/chats/!room/messages','POST',{text:'reply',replyToMessageID:'$reply'});assert.equal(sent[0].threadRoot,'$root');assert.equal(sent[0].replyTo,'$reply');
 await p.request('/v1/chats/!room/messages','POST',{text:'start thread',replyToMessageID:'$root'});assert.equal(sent[1].threadRoot,'$root');
 await p.request('/v1/chats/!room/messages','POST',{text:'thread view',threadRootID:'$root'});assert.equal(sent[2].threadRoot,'$root');
 p.uploads.set('photo',{bytes:Buffer.from('synthetic'),mime:'image/png',name:'photo.png'});
 await p.request('/v1/chats/!room/messages','POST',{threadRootID:'$root',attachment:{uploadID:'photo'}});assert.equal(media[0].threadRoot,'$root');
 await p.request('/v1/chats/!room/messages','POST',{text:'channel message'});assert.equal(sent[3].threadRoot,undefined);
});
