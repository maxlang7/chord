import test from 'node:test';
import assert from 'node:assert/strict';
import {searchMessages} from '../search.mjs';
test('unified search opens the correct source conversation and tolerates an offline provider',async()=>{
 const conversations=new Map([['matrix:!room',{id:'matrix:!room',source:'slack'}],['blueferry:one',{id:'blueferry:one',source:'iphone'}]]);
 const d=await searchMessages('hello',{matrix:async()=>({items:[{id:'$x',chatID:'!room',text:'hello',timestamp:'2026-11-08T00:00:00Z'}]}),blue:async()=>[{key:'one',messages:[{body:'hello there',handle:'a',timestamp:'2026-11-07T00:00:00Z'}]}],mailAccounts:()=>[{id:'home'},{id:'cmu'}],listMail:async id=>{if(id==='cmu')throw new Error('offline');return {items:[{id:'thread',subject:'hello',snippet:'Mail hello',date:'2026-11-08T01:00:00Z'}]}},directory:{conversations,enrichConversation:c=>c}});
 assert.equal(d.items.length,3);assert.equal(d.items[0].conversation.id,'mail:home:thread');assert.equal(d.items[1].conversation.id,'matrix:!room');assert.equal(d.items[2].message.id,'a');assert.deepEqual(d.errors,[{source:'mail:cmu',error:'offline'}]);
});
