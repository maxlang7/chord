import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {Worker} from 'node:worker_threads';
import {ArchiveStore,DAY,accountNamespace} from '../archive-store.mjs';
import {ReadCache} from '../read-cache.mjs';
import {readHistory,writeHistory} from '../history-file.mjs';
import {MatrixProvider} from '../matrix-provider.mjs';
const NOW=Date.parse('2026-11-08T12:00:00Z');
const message=(id,days,text=id)=>({id,text,timestamp:new Date(NOW-days*DAY).toISOString(),senderName:'Zoë',attachments:[]});

test('30-day archive is lossless, searchable by body and filename, and opening it does not reset age',()=>{
 const a=new ArchiveStore(':memory:',{now:()=>NOW});
 try{
  const old={...message('old',31,'Résumé molécules original'),attachments:[{fileName:'spectrum-final.pdf'}]};
  a.recordMessages('matrix:!r',[old,message('boundary',30),message('recent',1)],{title:'Research',source:'whatsapp'});
  assert.equal(a.compact(),1);assert.equal(a.compact(),0);
  assert.equal(a.stats().records.compressedVersions,1);
  assert.equal(a.search('resume molecules').items[0].message.text,old.text);
  const hit=a.search('spectrum final pdf').items[0];assert.equal(hit.compressed,true);
  const token=a.decodeToken(hit.conversation.id),saved=a.page(token.id,{around:token.anchor,version:token.version});
  assert.deepEqual(saved.items.map(({localArchive,compressed,historicalVersion,...m})=>m),[old]);
  assert.equal(a.stats().records.compressedVersions,1);assert.equal(a.search('" OR * : ;').items.length,0);
  assert.equal(hit.conversation.canSend,false);assert.equal(hit.conversation.kind,'archive');
 }finally{a.close()}
});
test('edited and redacted messages retain searchable old versions, and imports never overwrite a newer capture',()=>{
 const a=new ArchiveStore(':memory:',{now:()=>NOW});try{
  const old=message('same',40,'Unique original phrase');a.recordMessages('matrix:!r',[old]);
  a.recordMessages('matrix:!r',[old]);assert.equal(a.stats().records.versions,1);
  a.recordMessages('matrix:!r',[{...old,text:'Revised text'}]);
  a.recordMessages('matrix:!r',[old],{}, {historical:true});
  assert.equal(a.page('matrix:!r').items[0].text,'Revised text');
  a.recordMessages('matrix:!r',[{...old,text:'[Message deleted]',isDeleted:true}]);a.compact();
  const token=a.decodeToken(a.search('unique original').items[0].conversation.id);
  assert.equal(a.page(token.id,{around:token.anchor,version:token.version}).items[0].historicalVersion,true);
  assert.equal(a.page(token.id,{around:token.anchor,version:token.version}).items[0].text,old.text);
  assert.equal(a.stats().records.versions,3);
 }finally{a.close()}
});
test('saved files preserve exact bytes, share deduplicated storage, and remain available after compression',()=>{
 const a=new ArchiveStore(':memory:',{now:()=>NOW});try{
  a.recordMessages('matrix:!r',[message('m',31)]);
  const data=Buffer.from(Array.from({length:2048},(_,i)=>i%256)).toString('base64url');
  const value={data,mimeType:'application/octet-stream',fileName:'binary.dat'};
  a.capture('media:attachment:matrix:!r:m:0',value,{kind:'media',context:{conversationID:'matrix:!r',messageID:'m'}});
  a.capture('another-file',value,{kind:'media',context:{timestamp:NOW-31*DAY}});
  assert.equal(a.stats().resources.files,1);a.compact();assert.equal(a.stats().resources.compressedFiles,1);
  assert.deepEqual(a.attachment('matrix:!r','m'),value);assert.deepEqual(a.object('another-file'),value);
 }finally{a.close()}
});
test('archive namespaces isolate search, pages, files and forged result tokens',()=>{
 const a=new ArchiveStore(':memory:',{namespace:'a',now:()=>NOW});try{
  a.recordMessages('matrix:!r',[message('secret',40,'Private sentinel')]);a.capture('file',{data:'AA'},{kind:'media'});
  const hit=a.search('sentinel').items[0];a.namespace='b';
  assert.equal(a.search('sentinel').items.length,0);assert.equal(a.page('matrix:!r').items.length,0);assert.equal(a.object('file'),undefined);
  const d=a.decodeToken(hit.conversation.id);assert.equal(a.page(d.id,{version:d.version}).items.length,0);
  assert.throws(()=>a.decodeToken('archive:bad'),/Invalid/);
  const forged='archive:'+Buffer.from(JSON.stringify({id:'matrix:!r',kind:'matrix',version:1})).toString('base64url');
  assert.equal(a.decodeToken(forged).kind,undefined);
 }finally{a.close()}
});
test('saved versions of an attachment open their own bytes after its message is edited',()=>{
 const a=new ArchiveStore(':memory:');try{
  const m=message('m',40,'An attachment');const key='media:attachment:matrix:!r:m:0';
  for(const id of ['mxc://one','mxc://two']){
   a.recordMessages('matrix:!r',[{...m,attachments:[{id,fileName:'same-name.txt'}]}]);
   a.capture(key,{data:Buffer.from(id).toString('base64url')},{kind:'media',context:{conversationID:'matrix:!r',messageID:'m',attachmentID:id,index:0}});
  }
  a.compact();
  for(const id of ['mxc://one','mxc://two'])assert.equal(Buffer.from(a.attachment('matrix:!r','m',0,id).data,'base64url').toString(),id);
 }finally{a.close()}
});
test('archive search paginates and opens an exact older match even when timestamps tie',()=>{
 const a=new ArchiveStore(':memory:',{now:()=>NOW});try{
  a.recordMessages('matrix:!r',Array.from({length:140},(_,i)=>message(String(i),40,'Matching message '+i)));
  const first=a.search('matching',{limit:20}),second=a.search('matching',{limit:20,offset:first.nextOffset});
  assert.equal(first.items.length,20);assert.equal(first.hasMore,true);assert.equal(second.items.length,20);
  assert.notEqual(first.items[0].message.id,second.items[0].message.id);
  const target=a.search('matching',{limit:1,offset:110}).items[0],d=a.decodeToken(target.conversation.id);
  const p=a.page(d.id,{around:d.anchor,version:d.version,limit:10});assert.equal(p.items.at(-1).id,target.message.id);
  const before=JSON.parse(Buffer.from(p.oldestCursor,'base64url'));const older=a.page(d.id,{before,limit:10});
  assert.equal(older.items.some(m=>p.items.some(x=>x.id===m.id)),false);
 }finally{a.close()}
});
test('read cache retains everything, compresses old source dates, and restores objects from archive',()=>{
 const a=new ArchiveStore(':memory:',{now:()=>NOW});const c=new ReadCache(':memory:',{now:()=>NOW,archive:a,readBytes:1,mediaBytes:1});
 try{
  const value={items:[message('old',40,'Kept forever')]};const options={context:{conversationID:'matrix:!r'}};
  c.put('messages:matrix:!r:',value,options);c.put('another',{text:'retained'});
  assert.equal(c.compact(),1);assert.deepEqual(c.peek('messages:matrix:!r:').value,value);
  c.put('messages:matrix:!r:',value,options);assert.equal(c.db.prepare("SELECT codec FROM cache WHERE key=?").get('messages:matrix:!r:').codec,'br11');
  // A fresh cache can recover a permanently retained object without a provider.
  const restored=new ReadCache(':memory:',{archive:a});try{assert.deepEqual(restored.peek('messages:matrix:!r:').value,value)}finally{restored.close()}
  assert.ok(c.peek('another'));
 }finally{c.close();a.close()}
});
test('identical snapshots avoid repeated indexing, while returning to an earlier value updates the latest saved object',()=>{
 const a=new ArchiveStore(':memory:');try{
  let indexed=0;const record=a.recordMessages.bind(a);a.recordMessages=(...args)=>{indexed++;return record(...args)};
  const key='messages:matrix:!r:',first={items:[message('same',40,'First')]},second={items:[message('same',40,'Second')]};
  a.capture(key,first);a.capture(key,first);assert.equal(indexed,1);
  a.capture(key,second);a.capture(key,first);assert.equal(indexed,3);
  assert.deepEqual(a.object(key),first);assert.equal(a.page('matrix:!r').items[0].text,'First');assert.equal(a.stats().records.versions,2);
  a.capture(key,second);a.capture(key,first,{historical:true});assert.deepEqual(a.object(key),second);
 }finally{a.close()}
});
test('background import is idempotent, preserves legacy files, and compresses without losing search', {timeout:30000},async t=>{
 const dir=await mkdtemp(path.join(os.tmpdir(),'chord-archive-'));t.after(()=>rm(dir,{recursive:true,force:true}));
 const config={matrixAccount:{userId:'@fixture',deviceId:'fixture'}},namespace=accountNamespace(config);
 const original=JSON.stringify({chats:[['!r',{title:'Imported',accountID:'slack'}]],timelines:[['!r',[message('old',100,'Migration sentinel')]]],reactions:[]});
 await writeFile(path.join(dir,'matrix-timeline.json'),original);
 const cache=new ReadCache(path.join(dir,'read-cache.sqlite'),{namespace});cache.put('messages:mail:home:thread:',{items:[message('mail',100,'Email sentinel')]});cache.close();
 async function run(){const worker=new Worker(new URL('../archive-worker.mjs',import.meta.url),{workerData:{dataDir:dir,namespace}});try{await new Promise((resolve,reject)=>{worker.on('error',reject);worker.on('message',m=>{if(m.type==='error')reject(new Error(m.error));if(m.type==='maintenance')resolve()})})}finally{await worker.terminate()}}
 await run();const a=new ArchiveStore(path.join(dir,'archive.sqlite'),{namespace});try{
  assert.equal(a.stats().messages,2);assert.equal(a.stats().records.compressedVersions,2);assert.equal(a.search('sentinel').items.length,2);
  await run();assert.equal(a.stats().records.versions,2);assert.equal(a.meta('migration').state,'complete');
  assert.equal(await readFile(path.join(dir,'matrix-timeline.json'),'utf8'),original);
  await writeHistory(dir,{timelines:[]});assert.deepEqual(await readHistory(dir),{timelines:[]});
  assert.equal(await readFile(path.join(dir,'matrix-timeline.json'),'utf8'),original);
 }finally{a.close()}
});
test('live sync durably retains messages in unopened rooms and preserves text before redaction',()=>{
 const a=new ArchiveStore(':memory:',{now:()=>NOW});const p=new MatrixProvider({}, {userId:'@me'},{save:false,archive:a});
 try{
  p.chats.set('!r',{title:'Unopened',accountID:'whatsapp',participants:{items:[]}});
  p.onEvent({kind:'message',roomId:'!r',eventId:'$m',text:'Unopened sentinel',timestamp:NOW-40*DAY,sender:{userId:'@other'}});
  p.onEvent({kind:'event',type:'m.room.redaction',roomId:'!r',raw:{redacts:'$m'}});
  assert.equal(a.search('unopened sentinel').items.length,1);assert.equal(a.page('matrix:!r').items[0].isDeleted,true);assert.equal(a.stats().records.versions,2);
 }finally{a.close()}
});
test('startup snapshots omit old bodies only after preserving them, and cold messages still receive live redactions',async t=>{
 const dir=await mkdtemp(path.join(os.tmpdir(),'chord-cold-snapshot-'));t.after(()=>rm(dir,{recursive:true,force:true}));
 const a=new ArchiveStore(path.join(dir,'archive.sqlite'));const p=new MatrixProvider({}, {userId:'@me'},{dataDir:dir,archive:a});
 const now=Date.now(),old={...message('cold',40,'Cold snapshot sentinel'),timestamp:new Date(now-40*DAY).toISOString()},hot={...message('hot',1),timestamp:new Date(now-DAY).toISOString()};
 const original=JSON.stringify({timelines:[['!r',[old,hot]]]});await writeFile(path.join(dir,'matrix-timeline.json'),original);
 try{
  p.timelines.set('!r',[old,hot]);await p.save();
  assert.deepEqual((await readHistory(dir)).timelines,[['!r',[hot]]]);assert.equal(a.search('snapshot sentinel').items.length,1);
  assert.equal(await readFile(path.join(dir,'matrix-timeline.json'),'utf8'),original);
  const restarted=new MatrixProvider({}, {userId:'@me'},{save:false,archive:a});
  restarted.onEvent({kind:'event',type:'m.room.redaction',roomId:'!r',raw:{redacts:'cold'}});
  assert.equal(a.message('matrix:!r','cold').isDeleted,true);assert.equal(a.search('snapshot sentinel').items.length,1);
 }finally{a.close()}
});
