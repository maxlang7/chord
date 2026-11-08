import test from 'node:test';
import assert from 'node:assert/strict';
import {SqliteMatrixStore,recoverStateJSON} from '../matrix-store.mjs';
test('concurrent crypto writes replace complete values and retain independent keys',async()=>{
 const store=new SqliteMatrixStore(':memory:');
 try {
  await Promise.all(Array.from({length:100},(_,i)=>store.set('state',Buffer.from(JSON.stringify({i,text:'x'.repeat(100-i)})))));
  assert.equal(JSON.parse(Buffer.from(await store.get('state')).toString()).i,99);
  await store.set('session/a',new Uint8Array([1,2,3]));await store.set('session/b',new Uint8Array([4]));
  assert.deepEqual(await store.list('session/'),['session/a','session/b']);
  const data=await store.get('session/a');data[0]=9;assert.equal((await store.get('session/a'))[0],1);
  await store.delete('session/a');assert.equal(await store.get('session/a'),null);
 }finally{store.close()}
});
test('legacy state recovery keeps a complete JSON object and refuses a truncated object',()=>{
 const original=Buffer.from('{"a":"brace } and escaped \\"","b":{"c":1}}}}old data');
 const recovered=recoverStateJSON(original);
 assert.deepEqual(JSON.parse(recovered.toString()),{a:'brace } and escaped "',b:{c:1}});
 assert.ok(original.length>recovered.length);
 assert.throws(()=>recoverStateJSON(Buffer.from('{"missing":')),/incomplete/);
});
