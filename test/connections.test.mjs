import test from 'node:test';
import assert from 'node:assert/strict';
import {connectionStates} from '../connections.mjs';
test('separates reachable bridges from authenticated sessions and isolates provider failures',async()=>{
 const d=await connectionStates({config:{bridges:[{id:'discord',version:1},{id:'slack',version:3},{id:'whatsapp',version:3}]},bridgeInfo:async(_,id)=>id==='discord'?{Discord:{logged_in:true,connected:true}}:id==='slack'?{logins:[]}:{logins:{owner:[{id:'one',state_event:'CONNECTED'}]}},mailAccounts:()=>[{id:'home',email:'a@example.test'},{id:'cmu',email:'b@example.test'}],checkMail:async id=>{if(id==='cmu')throw new Error('Sign in again')},blue:async()=>({map:false,connectivity_detail:'Reconnect phone'})});
 assert.equal(d.items.length,6);
 const by=id=>d.items.find(i=>i.id===id);
 assert.equal(by('discord').connected,true);assert.equal(by('slack').state,'sign-in');assert.equal(by('whatsapp').connected,true);
 assert.equal(by('mail:home').connected,true);assert.equal(by('mail:cmu').connected,false);assert.equal(by('iphone').connected,false);
 assert.ok(!JSON.stringify(d).includes('owner'));assert.ok(d.checkedAt);
});
