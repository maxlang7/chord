import {DatabaseSync} from 'node:sqlite';

// This cache lives inside the owner-only private directory. Rows are scoped to
// the configured account identities, not OAuth tokens, so refreshes reuse it.
export class ReadCache {
 constructor(filename,{namespace='default',now=Date.now,readBytes=96*1024*1024,mediaBytes=384*1024*1024}={}){
  this.db=new DatabaseSync(filename);this.namespace=namespace;this.now=now;this.pending=new Map();this.errors=new Map();this.revision=0;this.limits={read:readBytes,media:mediaBytes};
  this.db.exec('PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS cache(namespace TEXT,key TEXT,value TEXT,updated INTEGER,touched INTEGER,kind TEXT,bytes INTEGER,PRIMARY KEY(namespace,key))');
 }
 peek(key){const row=this.db.prepare('SELECT value,updated FROM cache WHERE namespace=? AND key=?').get(this.namespace,key);if(!row)return null;this.db.prepare('UPDATE cache SET touched=? WHERE namespace=? AND key=?').run(this.now(),this.namespace,key);return {value:JSON.parse(row.value),updated:row.updated}}
 put(key,value,{kind='read',updated=this.now()}={}){
  const json=JSON.stringify(value);if(json===undefined)return;const bytes=Buffer.byteLength(json);if(bytes>32*1024*1024)return;
  this.db.prepare('INSERT OR REPLACE INTO cache VALUES (?,?,?,?,?,?,?)').run(this.namespace,key,json,updated,this.now(),kind,bytes);
  let total=Number(this.db.prepare('SELECT COALESCE(SUM(bytes),0) AS n FROM cache WHERE kind=?').get(kind).n);
  if(total>this.limits[kind])for(const row of this.db.prepare('SELECT namespace,key,bytes FROM cache WHERE kind=? ORDER BY touched').all(kind)){if(total<=this.limits[kind])break;this.db.prepare('DELETE FROM cache WHERE namespace=? AND key=?').run(row.namespace,row.key);total-=row.bytes}
 }
 invalidate(prefix=''){
  this.revision++;
  this.db.prepare('UPDATE cache SET updated=0 WHERE namespace=? AND substr(key,1,?)=?').run(this.namespace,prefix.length,prefix);
 }
 decorate(key,row){
  if(!row.value||Array.isArray(row.value)||typeof row.value!=='object')return row.value;
  return {...row.value,_cache:{cached:true,updatedAt:row.updated?new Date(row.updated).toISOString():null,refreshing:this.pending.has(key),error:this.errors.get(key)||null}};
 }
 refresh(key,loader,{kind='read'}={}){
  if(this.pending.has(key))return this.pending.get(key);
  const revision=this.revision;
  const promise=Promise.resolve().then(loader).then(value=>{
   // Keep a successful snapshot when one provider goes offline. Never turn a
   // useful inbox into a cached error or empty partial response.
   if(value?.errors?.length){this.errors.set(key,value.errors.map(e=>e.error||e.source).join('; '));if(this.peek(key))return this.peek(key).value}
   if(revision===this.revision)this.put(key,value,{kind});this.errors.delete(key);return value;
  }).catch(e=>{this.errors.set(key,e?.message||String(e));throw e}).finally(()=>{this.pending.delete(key)});
  this.pending.set(key,promise);return promise;
 }
 async get(key,loader,{maxAge=5000,fresh=false,kind='read',seed}={}){
  let row=this.peek(key);
  if(!row&&seed){const value=await seed();if(value!==undefined&&value!==null){this.put(key,value,{kind,updated:0});row=this.peek(key)}}
  if(row&&!fresh){if(this.now()-row.updated>=maxAge||row.updated===0)this.refresh(key,loader,{kind}).catch(()=>{});return this.decorate(key,row)}
  try{const value=await this.refresh(key,loader,{kind});const saved=this.peek(key);return this.errors.has(key)&&saved?this.decorate(key,saved):value}catch(e){if(row)return this.decorate(key,row);throw e}
 }
 async idle(){await Promise.allSettled([...this.pending.values()])}
 close(){this.db.close()}
}
