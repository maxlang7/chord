import {parentPort,workerData} from 'node:worker_threads';
import path from 'node:path';
import {ArchiveStore} from './archive-store.mjs';
import {ReadCache} from './read-cache.mjs';
import {allSavedHistory} from './history-file.mjs';
const {dataDir,namespace}=workerData;
const archive=new ArchiveStore(path.join(dataDir,'archive.sqlite'),{namespace});
const cache=new ReadCache(path.join(dataDir,'read-cache.sqlite'),{namespace});
const yieldWork=()=>new Promise(r=>setTimeout(r,5));
let maintaining=false;
async function migrate(){
 archive.meta('migration',{state:'importing',startedAt:new Date().toISOString()});
 let count=0;
 try{
  for await(const saved of allSavedHistory(dataDir)){
  const rooms=new Map(saved.chats||[]),reactions=new Map(saved.reactions||[]);
  for(const [id,messages] of saved.timelines||[]){
   const room=rooms.get(id)||{},cid='matrix:'+id;
   for(let i=0;i<messages.length;i+=100){archive.recordMessages(cid,messages.slice(i,i+100).map(m=>({...m,reactions:reactions.get(id+'|'+m.id)||m.reactions||[]})),{title:room.title||id,source:room.accountID||'matrix',kind:'chat',timestamp:room.lastActivity},{historical:true});count+=Math.min(100,messages.length-i);await yieldWork()}
  }
  }
 }catch(e){if(e.code!=='ENOENT')throw e}
 for(const row of cache.entries('')){
  const stamp=archive.capture(row.key,row.value,{kind:row.kind,historical:true,capturedAt:row.touched});
  cache.db.prepare('UPDATE cache SET event_at=? WHERE namespace=? AND key=?').run(stamp,namespace,row.key);await yieldWork();
 }
 archive.meta('migration',{state:'complete',importedMessageObservations:count,completedAt:new Date().toISOString()});
 parentPort.postMessage({type:'migration',state:'complete'});
}
async function maintain(){
 if(maintaining)return;maintaining=true;
 try{
  archive.meta('maintenance',{state:'compressing',startedAt:new Date().toISOString()});
  let total=0,changed;
  do{changed=archive.compact({limit:100})+cache.compact({limit:4});total+=changed;await yieldWork()}while(changed);
  archive.meta('maintenance',{state:'idle',compressedRecords:total,completedAt:new Date().toISOString()});parentPort.postMessage({type:'maintenance',state:'idle'});
 }catch(e){archive.meta('maintenance',{state:'error',error:e.message});parentPort.postMessage({type:'error',error:e.message})}
 finally{maintaining=false}
}
await migrate().then(maintain).catch(e=>{archive.meta('migration',{state:'error',error:e.message});parentPort.postMessage({type:'error',error:e.message})});
const timer=setInterval(maintain,10*60*1000);timer.unref();
parentPort.on('message',m=>{if(m==='maintain')maintain()});
