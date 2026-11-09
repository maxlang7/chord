import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import {spawn} from 'node:child_process';
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import {once} from 'node:events';
import os from 'node:os';
import path from 'node:path';

async function stop(child){if(child.exitCode!==null)return;const exited=once(child,'exit');child.kill('SIGTERM');await exited}
test('HTTP cache survives restart, serves reads during a stalled refresh, and invalidates after a send', {timeout:30000},async t=>{
 const dir=await mkdtemp(path.join(os.tmpdir(),'chord-api-cache-'));
 const config={apiKey:'synthetic-test-key',matrixAccount:{userId:'@test:chord.local',deviceId:'TEST',homeserver:'http://127.0.0.1:1'},mailAccounts:[]};
 await writeFile(path.join(dir,'config.json'),JSON.stringify(config));
 let text='First version',blocked=false,offline=false,waiting,release;
 const transport=http.createServer(async(req,res)=>{
  if(offline){res.writeHead(503,{'Content-Type':'application/json'});res.end(JSON.stringify({error:'Synthetic offline'}));return}
  if(req.method==='POST'){for await(const _ of req){}text='After send';res.end('{}');return}
  if(blocked){waiting?.();await new Promise(r=>release=r)}
  res.setHeader('Content-Type','application/json');res.end(JSON.stringify({items:[{id:'one',text,isSender:true,timestamp:'2026-11-08T20:00:00Z'}],hasMore:false}));
 });transport.listen(0,'127.0.0.1');await once(transport,'listening');
 const reservation=http.createServer();reservation.listen(0,'127.0.0.1');await once(reservation,'listening');const port=reservation.address().port;await new Promise(r=>reservation.close(r));
 const env={...process.env,PORT:String(port),CHORD_DATA_DIR:dir,CHORD_CONFIG:path.join(dir,'config.json'),CHORD_TEST_TRANSPORT_URL:'http://127.0.0.1:'+transport.address().port};
 let child;
 const start=()=>new Promise((resolve,reject)=>{
  child=spawn(process.execPath,['server.mjs'],{cwd:path.resolve(import.meta.dirname,'..'),env,stdio:['ignore','pipe','pipe']});
  const timer=setTimeout(()=>reject(new Error('Test API did not start')),10000);
  child.once('error',reject);child.stdout.on('data',b=>{if(b.toString().includes('Chord API on')){clearTimeout(timer);resolve()}});
 });
 t.after(async()=>{blocked=false;release?.();if(child)await stop(child);transport.closeAllConnections();await new Promise(r=>transport.close(r));await rm(dir,{recursive:true,force:true})});
 const route='/api/conversations/'+encodeURIComponent('matrix:!test')+'/messages';
 async function api(p,body){const r=await fetch('http://127.0.0.1:'+port+p,{method:body?'POST':'GET',headers:{Authorization:'Bearer '+config.apiKey,'Content-Type':'application/json'},body:body?JSON.stringify(body):undefined});assert.equal(r.status,200);return r.json()}
 await start();assert.equal((await api(route)).items[0].text,'First version');
 await stop(child);await start();const restored=await api(route);assert.equal(restored.items[0].text,'First version');assert.equal(restored._cache.cached,true);
 offline=true;const saved=await api(route+'?fresh=true');assert.equal(saved.items[0].text,'First version');assert.match(saved._cache.error,/offline/);offline=false;
 blocked=true;const started=new Promise(r=>waiting=r);const refresh=api(route+'?fresh=true');await started;
 const cached=await api(route);assert.equal(cached.items[0].text,'First version');assert.equal(cached._cache.refreshing,true);
 blocked=false;release();await refresh;
 await api('/api/conversations/'+encodeURIComponent('matrix:!test')+'/send',{text:'Synthetic local send'});
 assert.equal((await api(route+'?fresh=true')).items[0].text,'After send');
});
