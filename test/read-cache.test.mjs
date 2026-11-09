import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,rm} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {ReadCache} from '../read-cache.mjs';

test('saved reads survive restart, return before a blocked refresh, and remain readable offline',async t=>{
 const dir=await mkdtemp(path.join(os.tmpdir(),'chord-read-cache-'));t.after(()=>rm(dir,{recursive:true,force:true}));const file=path.join(dir,'cache.sqlite');
 let cache=new ReadCache(file,{now:()=>1000});cache.put('messages:one',{items:[{id:'old'}]});cache.close();
 cache=new ReadCache(file,{now:()=>10000});t.after(()=>cache.close());
 let finish,calls=0;const blocked=new Promise(r=>finish=r);
 const result=await cache.get('messages:one',()=>{calls++;return blocked});
 assert.equal(result.items[0].id,'old');assert.equal(result._cache.refreshing,true);
 await cache.get('messages:one',()=>{throw new Error('Duplicate refresh')});assert.equal(calls,1);
 finish({items:[{id:'new'}]});await cache.idle();assert.equal(cache.peek('messages:one').value.items[0].id,'new');
 cache.invalidate('messages:');const offline=await cache.get('messages:one',()=>{throw new Error('Network offline')},{fresh:true});
 assert.equal(offline.items[0].id,'new');assert.match(offline._cache.error,/offline/);
});
test('mutations prevent an old in-flight response from overwriting the invalidated snapshot',async()=>{
 const c=new ReadCache(':memory:');try{
  c.put('messages:one',{items:[{id:'original'}]});let finish;const work=c.refresh('messages:one',()=>new Promise(r=>finish=r));await Promise.resolve();
  c.invalidate('messages:');finish({items:[{id:'stale'}]});await work;
  assert.equal(c.peek('messages:one').value.items[0].id,'original');assert.equal(c.peek('messages:one').updated,0);
 }finally{c.close()}
});
test('account namespaces, media, indefinite retention, and seeded first reads are separate',async()=>{
 const c=new ReadCache(':memory:',{namespace:'owner-a',readBytes:200,mediaBytes:5000});try{
  c.put('media:one',{data:'file'},{kind:'media'});c.put('messages:a',{text:'x'.repeat(100)});c.put('messages:b',{text:'y'.repeat(100)});
  assert.ok(c.peek('messages:a'));assert.ok(c.peek('messages:b'));
  c.invalidate('messages:');assert.notEqual(c.peek('media:one').updated,0);
  c.namespace='owner-b';assert.equal(c.peek('media:one'),null);
  let finish;const seeded=await c.get('messages:c',()=>new Promise(r=>finish=r),{seed:()=>({items:[{id:'legacy'}]})});
  assert.equal(seeded.items[0].id,'legacy');finish({items:[{id:'fresh'}]});await c.idle();
 }finally{c.close()}
});
