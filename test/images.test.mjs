import test from 'node:test';
import assert from 'node:assert/strict';
import {MatrixProvider} from '../matrix-provider.mjs';

test('encrypted image metadata selects authenticated decryption instead of a raw media download', async () => {
 const file={url:'mxc://chord.local/encrypted-photo',key:{kty:'oct',k:'test-key'},iv:'test-iv',hashes:{sha256:'test-hash'},v:'v2'};
 let calls=0;
 const provider=new MatrixProvider({media:{download:async()=>{throw new Error('Encrypted images must not be fetched as plaintext')},downloadEncrypted:async({file:actual})=>{assert.deepEqual(actual,file);calls++;return {bytes:new Uint8Array([1,2,3])}}}},{userId:'@max:chord.local'},{save:false});
 const message=provider.normalizeMessage({roomId:'!room:chord.local',eventId:'$photo',timestamp:1000,sender:{userId:'@friend:chord.local'},attachments:[{encryptedFile:file,kind:'image',filename:'photo.jpg',contentType:'image/jpeg',width:640,height:480,size:12345}]});
 assert.equal(message.attachments[0].type,'img');
 assert.equal(message.attachments[0].mimeType,'image/jpeg');
 assert.deepEqual(message.attachments[0].size,{width:640,height:480});
 assert.equal(message.attachments[0].id,file.url);
 assert.equal(JSON.stringify(message).includes('test-key'),false,'media encryption keys are not returned in message metadata');
 const result=await provider.download(message.attachments[0].id);
 assert.equal(calls,1);
 assert.deepEqual([...result.bytes],[1,2,3]);
});
