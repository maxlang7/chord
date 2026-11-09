import test from 'node:test';
import assert from 'node:assert/strict';
import {MatrixProvider} from '../matrix-provider.mjs';
test('history returns while reaction recovery is stalled and duplicate page requests share work',async()=>{
 let finishHistory,finishRelations,calls=0;
 const p=new MatrixProvider({messages:{list:()=>{calls++;return new Promise(r=>finishHistory=r)}}},{userId:'@me:chord.local'},{save:false});
 p.chats.set('!room',{participants:{items:[]},accountID:'whatsapp'});
 p.raw=()=>new Promise(r=>finishRelations=r);
 const first=p.listMessages('!room'),second=p.listMessages('!room');
 assert.equal(calls,1);finishHistory({messages:[{roomId:'!room',eventId:'$one',text:'Fast text',sender:{userId:'@other:chord.local'},timestamp:1}],nextCursor:'older'});
 const [a,b]=await Promise.all([first,second]);assert.equal(a.items[0].text,'Fast text');assert.deepEqual(a,b);assert.ok(p.relationRefresh.has('!room'));
 finishRelations({chunk:[]});await p.relationRefresh.get('!room');
});
test('confirmed reactions update immediately and a stale recovery cannot undo them',async()=>{
 let release;
 const p=new MatrixProvider({reactions:{send:async()=>({eventId:'$reaction'}),redact:async()=>({})}},{userId:'@me:chord.local'},{save:false});
 p.raw=()=>new Promise(r=>release=r);
 const recovery=p.refreshReactions('!room',[{id:'$message'}]);
 await p.request('/v1/chats/'+encodeURIComponent('!room')+'/messages/'+encodeURIComponent('$message')+'/reactions','POST',{reactionKey:'👍'});
 release({chunk:[]});await recovery;
 assert.equal(p.reactions.get('!room|$message')[0].reactionKey,'👍');
 await p.request('/v1/chats/'+encodeURIComponent('!room')+'/messages/'+encodeURIComponent('$message')+'/reactions/'+encodeURIComponent('👍'),'DELETE');
 assert.equal(p.reactions.get('!room|$message').length,0);
});
