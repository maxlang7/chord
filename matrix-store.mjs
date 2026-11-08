import {DatabaseSync} from 'node:sqlite';
import {readFile,mkdir} from 'node:fs/promises';
import path from 'node:path';

// The upstream file adapter permits concurrent truncate/write operations. A
// SQLite row update gives each SDK key an atomic replacement, including crypto.
export class SqliteMatrixStore {
 constructor(filename){
  this.db=new DatabaseSync(filename);
  this.db.exec('PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value BLOB NOT NULL)');
  this.read=this.db.prepare('SELECT value FROM kv WHERE key=?');
  this.write=this.db.prepare('INSERT INTO kv(key,value) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value');
 }
 async get(key){const r=this.read.get(key);return r?new Uint8Array(r.value):null}
 async set(key,value){this.write.run(key,Buffer.from(value))}
 async delete(key){this.db.prepare('DELETE FROM kv WHERE key=?').run(key)}
 async list(prefix){return this.db.prepare('SELECT key FROM kv ORDER BY key').all().map(r=>r.key).filter(k=>k.startsWith(prefix))}
 close(){this.db.close()}
}
export function recoverStateJSON(bytes){
 const text=Buffer.from(bytes).toString('utf8');
 try{JSON.parse(text);return bytes}catch{}
 let depth=0,quoted=false,escaped=false;
 for(let i=0;i<text.length;i++){
  const c=text[i];
  if(quoted){if(escaped)escaped=false;else if(c==='\\')escaped=true;else if(c==='"')quoted=false;continue}
  if(c==='"')quoted=true;
  else if(c==='{'||c==='[')depth++;
  else if(c==='}'||c===']'){
   depth--;
   if(depth===0){const prefix=text.slice(0,i+1);JSON.parse(prefix);return Buffer.from(prefix)}
  }
 }
 throw new Error('Legacy Matrix state is incomplete. Original files are preserved for recovery.');
}
export async function openMatrixStore(dataDir){
 await mkdir(dataDir,{recursive:true,mode:0o700});
 const store=new SqliteMatrixStore(path.join(dataDir,'matrix-store.sqlite'));
 if(await store.get('chord/migration-complete'))return store;
 let index;
 try{index=JSON.parse(await readFile(path.join(dataDir,'matrix-crypto/index.json'),'utf8'))}catch(e){if(e.code==='ENOENT'){await store.set('chord/migration-complete',Buffer.from('new'));return store}store.close();throw e}
 // Read everything before committing. A failed import leaves no partial store.
 // Source files are never changed or removed, including recoverable trailing data.
 const rows=[];
 for(const [key,filename] of Object.entries(index)){
  if(path.basename(filename)!==filename)throw new Error('Invalid legacy Matrix filename');
  let value=await readFile(path.join(dataDir,'matrix-crypto',filename));
  if(key.endsWith('/state.json'))value=recoverStateJSON(value);
  rows.push([key,value]);
 }
 store.db.exec('BEGIN IMMEDIATE');
 try{for(const [key,value] of rows)store.write.run(key,value);store.write.run('chord/migration-complete',Buffer.from(String(rows.length)));store.db.exec('COMMIT')}
 catch(e){store.db.exec('ROLLBACK');store.close();throw e}
 return store;
}
