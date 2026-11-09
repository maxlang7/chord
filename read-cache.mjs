import {DatabaseSync} from 'node:sqlite';
import {brotliCompressSync,brotliDecompressSync,constants} from 'node:zlib';
import {unpackJSON,DAY,RETENTION_DAYS} from './archive-store.mjs';

// This cache lives inside the owner-only private directory. Rows are scoped to
// the configured account identities, not OAuth tokens, so refreshes reuse it.
export class ReadCache {
 constructor(filename,{namespace='default',now=Date.now,archive=null}={}){
  this.db=new DatabaseSync(filename);this.namespace=namespace;this.now=now;this.pending=new Map();this.errors=new Map();this.revision=0;this.archive=archive;
  this.db.exec('PRAGMA journal_mode=WAL; PRAGMA busy_timeout=3000; CREATE TABLE IF NOT EXISTS cache(namespace TEXT,key TEXT,value TEXT,updated INTEGER,touched INTEGER,kind TEXT,bytes INTEGER,PRIMARY KEY(namespace,key))');
  const columns=new Set(this.db.prepare('PRAGMA table_info(cache)').all().map(c=>c.name));
  if(!columns.has('codec'))this.db.exec("ALTER TABLE cache ADD COLUMN codec TEXT NOT NULL DEFAULT 'plain'");
  if(!columns.has('event_at')){this.db.exec('ALTER TABLE cache ADD COLUMN event_at INTEGER NOT NULL DEFAULT 0');this.db.exec('UPDATE cache SET event_at=CASE WHEN updated>0 THEN updated ELSE touched END')}

 }
 peek(key){
  const row=this.db.prepare('SELECT value,updated,codec FROM cache WHERE namespace=? AND key=?').get(this.namespace,key);
  if(!row){const value=this.archive?.object(key);return value===undefined?null:{value,updated:0}}
  this.db.prepare('UPDATE cache SET touched=? WHERE namespace=? AND key=?').run(this.now(),this.namespace,key);return {value:unpackJSON(row.value,row.codec),updated:row.updated};
 }
 entries(prefix=''){
  return this.db.prepare('SELECT key,value,updated,touched,kind,codec,event_at FROM cache WHERE namespace=? AND substr(key,1,?)=? ORDER BY updated').all(this.namespace,prefix.length,prefix).map(row=>({...row,value:unpackJSON(row.value,row.codec)}));
 }
 put(key,value,{kind='read',updated=this.now(),context={}}={}){
  const json=JSON.stringify(value);if(json===undefined)return;
  const eventAt=this.archive?.capture(key,value,{kind,context,historical:updated===0})??this.now();
  const bytes=Buffer.byteLength(json);if(bytes>32*1024*1024)return;
  const old=this.db.prepare('SELECT value,codec FROM cache WHERE namespace=? AND key=?').get(this.namespace,key);
  // Do not expand a compressed old object simply because it was opened again.
  if(old?.codec==='br11'&&JSON.stringify(unpackJSON(old.value,old.codec))===json){this.db.prepare('UPDATE cache SET updated=?,touched=? WHERE namespace=? AND key=?').run(updated,this.now(),this.namespace,key);return}
  this.db.prepare("INSERT OR REPLACE INTO cache(namespace,key,value,updated,touched,kind,bytes,codec,event_at) VALUES (?,?,?,?,?,?,?,'plain',?)").run(this.namespace,key,Buffer.from(json),updated,this.now(),kind,bytes,eventAt);
 }
 compact({limit=20}={}){
  const rows=this.db.prepare("SELECT key,value FROM cache WHERE namespace=? AND codec='plain' AND event_at<? LIMIT ?").all(this.namespace,this.now()-RETENTION_DAYS*DAY,limit);
  for(const r of rows){const raw=Buffer.from(r.value);const packed=brotliCompressSync(raw,{params:{[constants.BROTLI_PARAM_QUALITY]:11}});if(!brotliDecompressSync(packed).equals(raw))throw new Error('Cache compression verification failed; original kept');this.db.prepare("UPDATE cache SET value=?,codec='br11',bytes=? WHERE namespace=? AND key=? AND codec='plain'").run(packed,packed.length,this.namespace,r.key)}
  return rows.length;
 }

 invalidate(prefix=''){
  this.revision++;
  this.db.prepare('UPDATE cache SET updated=0 WHERE namespace=? AND substr(key,1,?)=?').run(this.namespace,prefix.length,prefix);
 }
 decorate(key,row){
  if(!row.value||Array.isArray(row.value)||typeof row.value!=='object')return row.value;
  return {...row.value,_cache:{cached:true,updatedAt:row.updated?new Date(row.updated).toISOString():null,refreshing:this.pending.has(key),error:this.errors.get(key)||null}};
 }
 refresh(key,loader,{kind='read',context={}}={}){
  if(this.pending.has(key))return this.pending.get(key);
  const revision=this.revision;
  const promise=Promise.resolve().then(loader).then(value=>{
   // Keep a successful snapshot when one provider goes offline. Never turn a
   // useful inbox into a cached error or empty partial response.
   if(value?.errors?.length){this.errors.set(key,value.errors.map(e=>e.error||e.source).join('; '));if(this.peek(key))return this.peek(key).value}
   if(revision===this.revision)this.put(key,value,{kind,context});this.errors.delete(key);return value;
  }).catch(e=>{this.errors.set(key,e?.message||String(e));throw e}).finally(()=>{this.pending.delete(key)});
  this.pending.set(key,promise);return promise;
 }
 async get(key,loader,{maxAge=5000,fresh=false,kind='read',seed,context={}}={}){
  let row=this.peek(key);
  if(!row&&seed){const value=await seed();if(value!==undefined&&value!==null){this.put(key,value,{kind,updated:0,context});row=this.peek(key)}}
  if(row&&!fresh){if(this.now()-row.updated>=maxAge||row.updated===0)this.refresh(key,loader,{kind,context}).catch(()=>{});return this.decorate(key,row)}
  try{const value=await this.refresh(key,loader,{kind,context});const saved=this.peek(key);return this.errors.has(key)&&saved?this.decorate(key,saved):value}catch(e){if(row)return this.decorate(key,row);throw e}
 }
 async idle(){await Promise.allSettled([...this.pending.values()])}
 close(){this.db.close()}
}
