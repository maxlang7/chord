import test from 'node:test';
import assert from 'node:assert/strict';
import {avatarService} from '../avatars.mjs';
const account={homeserver:'http://127.0.0.1:4808',accessToken:'local-test'};
const contact={id:'person_test',identities:[{key:'@discord_123:chord.local'}]};

test('profile photos use local authenticated media, cache results, and return only image bytes',async()=>{
 const calls=[];const get=avatarService({fetcher:async(url,options)=>{calls.push(url);assert.equal(options.headers.Authorization,'Bearer local-test');return url.pathname.endsWith('/avatar_url')?Response.json({avatar_url:'mxc://chord.local/photo'}):new Response(new Uint8Array([1,2,3]),{headers:{'content-type':'image/png'}})}});
 const result=await get(contact,account);assert.equal(result.data,Buffer.from([1,2,3]).toString('base64url'));assert.equal(result.mimeType,'image/png');
 assert.deepEqual(await get(contact,account),result);assert.equal(calls.length,2);assert.ok(calls.every(url=>url.origin==='http://127.0.0.1:4808'));
});
test('profile photos never proxy arbitrary external URLs or remote Matrix media',async()=>{
 for(const avatar of ['https://tracking.example/photo','mxc://remote.example/photo','file:///etc/passwd']){
  let calls=0;const get=avatarService({fetcher:async()=>{calls++;return Response.json({avatar_url:avatar})}});assert.equal(await get(contact,account),null);assert.equal(calls,1);
 }
 let calls=0;const get=avatarService({fetcher:async()=>{calls++;throw new Error('must not fetch')}});assert.equal(await get({id:'mail',identities:[{key:'mailto:test@example.com'}]},account),null);assert.equal(calls,0);
});
test('oversized and active content profile images are refused',async()=>{
 for(const headers of [{'content-type':'text/html'},{'content-type':'image/svg+xml'},{'content-type':'image/png','content-length':String(3*1024*1024)}]){
  const get=avatarService({fetcher:async(url)=>url.pathname.endsWith('/avatar_url')?Response.json({avatar_url:'mxc://chord.local/photo'}):new Response('not an accepted photo',{headers})});assert.equal(await get(contact,account),null);
 }
});
