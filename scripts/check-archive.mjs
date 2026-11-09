// Verify retained legacy observations and the checksums of every stored payload.
// Output contains counts only. Account secrets and message contents stay local.
import {readFile} from 'node:fs/promises';
import path from 'node:path';
import {brotliDecompressSync} from 'node:zlib';
import {ArchiveStore,accountNamespace,digest} from '../archive-store.mjs';
import {ReadCache} from '../read-cache.mjs';
import {allSavedHistory} from '../history-file.mjs';
const dataDir=path.resolve(process.argv[2]||path.join(import.meta.dirname,'../private'));
const config=JSON.parse(await readFile(path.join(dataDir,'config.json'),'utf8'));
const namespace=accountNamespace(config),archive=new ArchiveStore(path.join(dataDir,'archive.sqlite'),{namespace});
const cache=new ReadCache(path.join(dataDir,'read-cache.sqlite'),{namespace});
const report={legacyObservations:0,missingObservations:0,checkedPayloads:0,checksumFailures:0,checkedFiles:0,fileFailures:0};
try{
 const find=archive.db.prepare('SELECT 1 FROM records WHERE namespace=? AND conversation_id=? AND message_id=? AND digest=?');
 for await(const saved of allSavedHistory(dataDir)){
  const reactions=new Map(saved.reactions||[]);
  for(const [id,messages] of saved.timelines||[])for(const m of messages){
   const value={...m,reactions:reactions.get(id+'|'+m.id)||m.reactions||[]};value.id=String(value.id);
   report.legacyObservations++;if(!find.get(namespace,'matrix:'+id,value.id,digest(JSON.stringify(value))))report.missingObservations++;
  }
 }
 for(const table of ['records','blobs','resources'])for(const row of archive.db.prepare(`SELECT payload,codec,digest FROM ${table} WHERE namespace=?`).all(namespace)){
  report.checkedPayloads++;try{const raw=row.codec==='br11'?brotliDecompressSync(row.payload):Buffer.from(row.payload);if(digest(raw)!==row.digest)report.checksumFailures++}catch{report.checksumFailures++}
 }
 for(const row of cache.entries('media:'))if(typeof row.value?.data==='string'){
  report.checkedFiles++;const retained=archive.object(row.key);
  if(!retained?.data||digest(Buffer.from(retained.data,'base64url'))!==digest(Buffer.from(row.value.data,'base64url')))report.fileFailures++;
 }
 console.log(JSON.stringify(report));if(report.missingObservations||report.checksumFailures||report.fileFailures)process.exitCode=1;
}finally{cache.close();archive.close()}
