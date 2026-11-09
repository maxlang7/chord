import test from 'node:test';import assert from 'node:assert/strict';import {isStatusConversation} from '../status-conversations.mjs';
const owner='@max:chord.local';
test('bridge-control conversations are excluded without hiding human messages',()=>{
 for(const network of ['slack','discord','whatsapp','blueferry'])assert.equal(isStatusConversation({source:'matrix',kind:'chat',participants:[owner,'@'+network+'bot:chord.local']},owner),true);
 assert.equal(isStatusConversation({source:'blueferry',kind:'chat'},owner),true);
 assert.equal(isStatusConversation({source:'matrix',title:'WhatsApp connection',participants:[owner]},owner),true);
 for(const c of [{source:'iphone',kind:'phone',title:'BlueFerry status updates'},{source:'slack',kind:'chat',title:'BlueFerry status updates'},{source:'matrix',participants:[owner,'@friend:chord.local','@blueferrybot:chord.local']},{source:'matrix',title:'BlueFerry',participants:[owner,'@friend:chord.local']}])assert.equal(isStatusConversation(c,owner),false);
});
