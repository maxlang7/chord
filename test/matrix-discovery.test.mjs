import test from 'node:test';
import assert from 'node:assert/strict';
import {MatrixProvider} from '../matrix-provider.mjs';

test('discovers new rooms and accepts only local bridge invitations', async () => {
 const account={userId:'@max:chord.local'};
 const invited=(sender)=>({invite_state:{events:[{type:'m.room.member',state_key:account.userId,sender,content:{membership:'invite'}}]}});
 const joined={state:{events:[{type:'m.room.name',content:{name:'Connected chat'}},{type:'m.bridge',content:{protocol:{id:'discordgo',displayname:'Discord'}}}]}};
 let joinedBridge=false,showNewRoom=false;
 const joins=[];
 const client={raw:{request:async({method,path})=>{
  if(method==='POST'){joins.push(path);joinedBridge=true;return {status:200,body:{}}}
  assert.ok(path.endsWith('/sync'));
  return {status:200,body:{rooms:{join:{...(joinedBridge?{'!discord:chord.local':joined}:{}),...(showNewRoom?{'!new:chord.local':joined}:{})},invite:{...(!joinedBridge?{'!discord:chord.local':invited('@discord_1:chord.local')}:{}) ,'!remote:other.example':invited('@discord_2:other.example')}}}};
 }}};
 const provider=new MatrixProvider(client,account,{save:false});
 await provider.refreshRooms();
 await provider.inviteWork;
 await provider.refreshRooms();
 assert.equal(joins.length,1);
 assert.equal(provider.chats.get('!discord:chord.local').accountID,'discord');
 assert.ok(!provider.chats.has('!remote:other.example'));
 showNewRoom=true;provider.lastRoomRefresh=0;
 const result=await provider.request('/v1/chats?accountIDs=discord');
 assert.equal(result.items.length,2);
 assert.equal(joins.length,1);
});
