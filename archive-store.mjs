import {DatabaseSync} from 'node:sqlite';
import {createHash} from 'node:crypto';
import {brotliCompressSync,brotliDecompressSync,constants} from 'node:zlib';

export const RETENTION_DAYS=30;
export const DAY=86400000;
export const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
export function accountNamespace(config){return digest(JSON.stringify([config.matrixAccount?.homeserver,config.matrixAccount?.userId,config.matrixAccount?.deviceId,(config.mailAccounts||[]).map(a=>[a.id,a.email])]))}
export function unpackJSON(value,codec='plain'){
 const bytes=Buffer.isBuffer(value)||value instanceof Uint8Array?Buffer.from(value):Buffer.from(value);
 return JSON.parse((codec==='br11'?brotliDecompressSync(bytes):bytes).toString());
}
export function eventTime(value,fallback=Date.now()){
 if(typeof value==='number'&&Number.isFinite(value)&&value>0)return value<1e11?value*1000:value;
 const n=Date.parse(value);return Number.isFinite(n)&&n>0?n:fallback;
}
export function sourceFor(id){return id.startsWith('mail:')?id.split(':').slice(0,2).join(':'):id.startsWith('blueferry:')?'iphone':'matrix'}
function inferredConversation(key){if(!key.startsWith('messages:'))return null;const s=key.slice(9);return s.slice(0,s.lastIndexOf(':'))}
function literalQuery(text){const words=String(text).match(/[\p{L}\p{N}_]+/gu)||[];return words.slice(0,24).map(w=>'"'+w.replaceAll('"','""')+'"').join(' AND ')}

// Append-only captured versions. Compression changes representation, never content.
// FTS stores token positions without duplicating the original plaintext bodies.
export class ArchiveStore {
 constructor(filename,{namespace='default',now=Date.now}={}){
  this.db=new DatabaseSync(filename);this.namespace=namespace;this.now=now;
  this.db.exec(`PRAGMA journal_mode=WAL; PRAGMA busy_timeout=3000;
   CREATE TABLE IF NOT EXISTS conversations(namespace TEXT,id TEXT,source TEXT,title TEXT,payload TEXT,PRIMARY KEY(namespace,id));
   CREATE TABLE IF NOT EXISTS records(seq INTEGER PRIMARY KEY,namespace TEXT,conversation_id TEXT,message_id TEXT,digest TEXT,event_at INTEGER,captured_at INTEGER,payload BLOB,codec TEXT,raw_bytes INTEGER,UNIQUE(namespace,conversation_id,message_id,digest));
   CREATE TABLE IF NOT EXISTS current_messages(namespace TEXT,conversation_id TEXT,message_id TEXT,seq INTEGER,event_at INTEGER,PRIMARY KEY(namespace,conversation_id,message_id));
   CREATE INDEX IF NOT EXISTS message_dates ON current_messages(namespace,conversation_id,event_at);
   CREATE INDEX IF NOT EXISTS record_age ON records(namespace,codec,event_at);
   CREATE TABLE IF NOT EXISTS blobs(namespace TEXT,digest TEXT,event_at INTEGER,captured_at INTEGER,payload BLOB,codec TEXT,raw_bytes INTEGER,PRIMARY KEY(namespace,digest));
   CREATE TABLE IF NOT EXISTS resources(namespace TEXT,digest TEXT,event_at INTEGER,captured_at INTEGER,payload BLOB,codec TEXT,raw_bytes INTEGER,PRIMARY KEY(namespace,digest));
   CREATE TABLE IF NOT EXISTS objects(seq INTEGER PRIMARY KEY,namespace TEXT,key TEXT,digest TEXT,event_at INTEGER,captured_at INTEGER,kind TEXT,UNIQUE(namespace,key,digest));
   CREATE INDEX IF NOT EXISTS object_keys ON objects(namespace,key,seq DESC);
   CREATE TABLE IF NOT EXISTS archive_meta(namespace TEXT,key TEXT,value TEXT,PRIMARY KEY(namespace,key));
   CREATE VIRTUAL TABLE IF NOT EXISTS message_fts USING fts5(body,title,sender,filenames,content='',contentless_delete=1,tokenize='unicode61 remove_diacritics 2');`);
 }
 meta(key,value){if(value!==undefined)this.db.prepare('INSERT OR REPLACE INTO archive_meta VALUES (?,?,?)').run(this.namespace,key,JSON.stringify(value));const r=this.db.prepare('SELECT value FROM archive_meta WHERE namespace=? AND key=?').get(this.namespace,key);return r?JSON.parse(r.value):null}
 conversation(c){
  if(!c?.id)return;
  const previous=this.getConversation(c.id)||{};
  const value={...previous,...c,source:c.source||previous.source||sourceFor(c.id),title:c.title&&c.title!==c.id?c.title:previous.title||c.id};
  this.db.prepare('INSERT OR REPLACE INTO conversations VALUES (?,?,?,?,?)').run(this.namespace,c.id,value.source,value.title,JSON.stringify(value));
  return value;
 }
 getConversation(id){const r=this.db.prepare('SELECT payload FROM conversations WHERE namespace=? AND id=?').get(this.namespace,id);return r?JSON.parse(r.payload):null}
 message(id,messageID){if(typeof messageID!=='string')return null;const r=this.db.prepare('SELECT r.payload,r.codec FROM current_messages c JOIN records r ON r.seq=c.seq WHERE c.namespace=? AND c.conversation_id=? AND c.message_id=?').get(this.namespace,id,messageID);return r?unpackJSON(r.payload,r.codec):null}
 recordMessages(conversationID,items,conversation={}, {historical=false}={}){
  if(!items?.length)return;
  const c=this.conversation({id:conversationID,...conversation});
  this.db.exec('BEGIN IMMEDIATE');
  try{
   for(const original of items){
    const m={...original};m.id=String(m.id||m.handle||digest(JSON.stringify([m.timestamp,m.senderID,m.text||m.body])).slice(0,24));
    const bytes=Buffer.from(JSON.stringify(m)),hash=digest(bytes),stamp=eventTime(m.timestamp||m.date,this.now());
    const inserted=this.db.prepare("INSERT OR IGNORE INTO records(namespace,conversation_id,message_id,digest,event_at,captured_at,payload,codec,raw_bytes) VALUES (?,?,?,?,?,?,?,'plain',?)").run(this.namespace,conversationID,m.id,hash,stamp,this.now(),bytes,bytes.length);
    const seq=inserted.changes?Number(inserted.lastInsertRowid):this.db.prepare('SELECT seq FROM records WHERE namespace=? AND conversation_id=? AND message_id=? AND digest=?').get(this.namespace,conversationID,m.id,hash).seq;
    if(inserted.changes)this.db.prepare('INSERT INTO message_fts(rowid,body,title,sender,filenames) VALUES (?,?,?,?,?)').run(seq,String(m.text||m.body||'')+' '+String(m.subject||''),c.title,String(m.senderName||m.from||m.senderID||''),(m.attachments||[]).map(a=>a.fileName||a.filename||'').join(' '));
    this.db.prepare(historical?'INSERT OR IGNORE INTO current_messages VALUES (?,?,?,?,?)':'INSERT OR REPLACE INTO current_messages VALUES (?,?,?,?,?)').run(this.namespace,conversationID,m.id,seq,stamp);
   }
   this.db.exec('COMMIT');
  }catch(e){this.db.exec('ROLLBACK');throw e}
 }
 capture(key,value,{kind='read',context={},historical=false,capturedAt=this.now()}={}){
  let conversationID=context.conversationID||inferredConversation(key),messageID=context.messageID;
  if(kind==='media'&&!conversationID&&(key.startsWith('media:attachment:')||key.startsWith('media:mail:'))){
   const suffix=key.slice(key.startsWith('media:attachment:')?17:11);
   const c=this.db.prepare("SELECT id FROM conversations WHERE namespace=? AND substr(?,1,length(id)+1)=id||':' ORDER BY length(id) DESC LIMIT 1").get(this.namespace,suffix);
   if(c){conversationID=c.id;const tail=suffix.slice(c.id.length+1);messageID=tail.slice(0,tail.lastIndexOf(':'))}
  }
  let stamp=eventTime(context.timestamp,capturedAt);
  if(conversationID&&Array.isArray(value?.items)){
   const dates=value.items.map(m=>eventTime(m.timestamp||m.date,capturedAt));if(dates.length)stamp=Math.max(...dates);
   this.recordMessages(conversationID,value.items,context.conversation||{source:context.source||sourceFor(conversationID)},{historical});
  }else if(key.startsWith('inbox:')&&Array.isArray(value?.items)){for(const c of value.items)this.conversation(c)}
  else if(key.startsWith('blue:ListThreads:')&&Array.isArray(value)){
   for(const t of value){const id='blueferry:'+t.key;this.recordMessages(id,(t.messages||[]).map(m=>({...m,id:m.handle,text:m.body,isSender:!!m.outgoing,senderID:m.address,senderName:m.sender||t.name})),{id,title:t.name,source:'iphone',kind:'phone',timestamp:t.last_ts,participants:t.recipients||[]},{historical})}
  }
  if(kind==='media'&&messageID&&conversationID){const r=this.db.prepare('SELECT event_at FROM current_messages WHERE namespace=? AND conversation_id=? AND message_id=?').get(this.namespace,conversationID,messageID);if(r)stamp=r.event_at}
  let stored=value;
  if(kind==='media'&&typeof value?.data==='string'){
   const raw=Buffer.from(value.data,'base64url'),resource=digest(raw);
   this.db.prepare("INSERT OR IGNORE INTO resources VALUES (?,?,?,?,?,'raw',?)").run(this.namespace,resource,stamp,capturedAt,raw,raw.length);
   const {data,...metadata}=value;stored={...metadata,_resource:resource};
  }
  const bytes=Buffer.from(JSON.stringify(stored)),hash=digest(bytes);
  this.db.prepare("INSERT OR IGNORE INTO blobs VALUES (?,?,?,?,?,'plain',?)").run(this.namespace,hash,stamp,capturedAt,bytes,bytes.length);
  this.db.prepare('INSERT OR IGNORE INTO objects(namespace,key,digest,event_at,captured_at,kind) VALUES (?,?,?,?,?,?)').run(this.namespace,key,hash,stamp,capturedAt,kind);
  return stamp;
 }
 object(key){const r=this.db.prepare('SELECT b.payload,b.codec FROM objects o JOIN blobs b ON b.namespace=o.namespace AND b.digest=o.digest WHERE o.namespace=? AND o.key=? ORDER BY o.seq DESC LIMIT 1').get(this.namespace,key);if(!r)return undefined;const value=unpackJSON(r.payload,r.codec);if(value?._resource){const b=this.db.prepare('SELECT payload,codec FROM resources WHERE namespace=? AND digest=?').get(this.namespace,value._resource);if(!b)throw new Error('Saved attachment bytes are missing');const {_resource,...metadata}=value;return {...metadata,data:(b.codec==='br11'?brotliDecompressSync(b.payload):Buffer.from(b.payload)).toString('base64url')}}return value}
 decodeToken(id){
  if(!id.startsWith('archive:')||id.length>12000)throw Object.assign(new Error('Invalid archive conversation'),{status:400});
  let d;try{d=JSON.parse(Buffer.from(id.slice(8),'base64url').toString())}catch{throw Object.assign(new Error('Invalid archive conversation'),{status:400})}
  if(!d||typeof d.id!=='string'||d.id.length>4096||(d.anchor!==undefined&&typeof d.anchor!=='string')||(d.version!==undefined&&(!Number.isSafeInteger(d.version)||d.version<1)))throw Object.assign(new Error('Invalid archive conversation'),{status:400});return {id:d.id,anchor:d.anchor,version:d.version};
 }
 attachment(id,messageID,index=0,attachmentID){
  if(typeof messageID!=='string'||!Number.isSafeInteger(index)||index<0)throw Object.assign(new Error('Invalid saved attachment'),{status:400});
  const r=this.db.prepare('SELECT r.payload,r.codec FROM current_messages c JOIN records r ON r.seq=c.seq WHERE c.namespace=? AND c.conversation_id=? AND c.message_id=?').get(this.namespace,id,messageID);
  const a=r?unpackJSON(r.payload,r.codec).attachments?.[index]:null;
  const key=id.startsWith('mail:')?'media:mail:'+id+':'+messageID+':'+(attachmentID||a?.id):'media:attachment:'+id+':'+messageID+':'+index;
  return this.object(key);
 }
 page(id,{before=null,around=null,version=null,limit=50}={}){
  limit=Math.max(1,Math.min(200,Number(limit)||50));
  let rows;
  if(around){
   const anchor=this.db.prepare('SELECT event_at,seq FROM current_messages WHERE namespace=? AND conversation_id=? AND message_id=?').get(this.namespace,id,around);
   rows=this.db.prepare('SELECT r.* FROM current_messages c JOIN records r ON r.seq=c.seq WHERE c.namespace=? AND c.conversation_id=? AND (c.event_at<? OR (c.event_at=? AND c.seq<=?)) ORDER BY c.event_at DESC,c.seq DESC LIMIT ?').all(this.namespace,id,anchor?.event_at??Number.MAX_SAFE_INTEGER,anchor?.event_at??Number.MAX_SAFE_INTEGER,anchor?.seq??Number.MAX_SAFE_INTEGER,limit);
  }else rows=this.db.prepare('SELECT r.* FROM current_messages c JOIN records r ON r.seq=c.seq WHERE c.namespace=? AND c.conversation_id=? AND (c.event_at<? OR (c.event_at=? AND c.seq<?)) ORDER BY c.event_at DESC,c.seq DESC LIMIT ?').all(this.namespace,id,before?.at??Number.MAX_SAFE_INTEGER,before?.at??Number.MAX_SAFE_INTEGER,before?.seq??Number.MAX_SAFE_INTEGER,limit);
  const oldest=rows.at(-1),items=rows.reverse().map(r=>({...unpackJSON(r.payload,r.codec),localArchive:true,compressed:r.codec==='br11'}));
  if(version){const r=this.db.prepare('SELECT * FROM records WHERE namespace=? AND conversation_id=? AND seq=?').get(this.namespace,id,version);if(r){const current=this.db.prepare('SELECT seq FROM current_messages WHERE namespace=? AND conversation_id=? AND message_id=?').get(this.namespace,id,r.message_id);const m={...unpackJSON(r.payload,r.codec),localArchive:true,historicalVersion:current?.seq!==r.seq};const i=items.findIndex(x=>x.id===m.id);if(i>=0)items[i]=m;else items.push(m)}}
  const hasMore=!!oldest&&!!this.db.prepare('SELECT 1 FROM current_messages WHERE namespace=? AND conversation_id=? AND (event_at<? OR (event_at=? AND seq<?)) LIMIT 1').get(this.namespace,id,oldest.event_at,oldest.event_at,oldest.seq);
  return {items,hasMore,oldestCursor:hasMore?Buffer.from(JSON.stringify({at:oldest.event_at,seq:oldest.seq})).toString('base64url'):null,archive:true};
 }
 search(query,{limit=60,offset=0}={}){
  limit=Math.max(1,Math.min(200,Math.trunc(Number(limit))||60));offset=Math.max(0,Math.min(1000000,Math.trunc(Number(offset))||0));
  const match=literalQuery(query);if(!match)return {items:[],hasMore:false,scope:'All locally retained messages, including compressed archives'};
  const rows=this.db.prepare(`WITH hits AS (
   SELECT r.*,ROW_NUMBER() OVER (PARTITION BY r.conversation_id,r.message_id ORDER BY r.captured_at DESC,r.seq DESC) AS n
   FROM message_fts JOIN records r ON r.seq=message_fts.rowid WHERE message_fts MATCH ? AND r.namespace=?
  ) SELECT * FROM hits WHERE n=1 ORDER BY event_at DESC,seq DESC LIMIT ? OFFSET ?`).all(match,this.namespace,Math.min(200,Number(limit)||60)+1,Math.max(0,Number(offset)||0));
  const hasMore=rows.length>limit;
  const items=rows.slice(0,limit).map(r=>{
   const original=this.getConversation(r.conversation_id)||{id:r.conversation_id,title:r.conversation_id,source:sourceFor(r.conversation_id)};
   const token=Buffer.from(JSON.stringify({id:r.conversation_id,anchor:r.message_id,version:r.seq})).toString('base64url');
   return {conversation:{...original,id:'archive:'+token,kind:'archive',canSend:false,canReply:false,canReact:false},message:{...unpackJSON(r.payload,r.codec),localArchive:true},localArchive:true,archived:r.event_at<this.now()-RETENTION_DAYS*DAY,compressed:r.codec==='br11'};
  });
  return {items,hasMore,nextOffset:hasMore?offset+limit:null,scope:'All locally retained messages, including compressed archives'};
 }
 compact({limit=100}={}){
  const cutoff=this.now()-RETENTION_DAYS*DAY;let changed=0;
  for(const table of ['records','blobs','resources']){
   const rows=this.db.prepare(`SELECT rowid AS row_id,* FROM ${table} WHERE namespace=? AND codec IN ('plain','raw') AND event_at<? LIMIT ?`).all(this.namespace,cutoff,limit);
   for(const row of rows){
    const raw=Buffer.from(row.payload);if(digest(raw)!==row.digest)throw new Error('Archive checksum mismatch; original record kept');
    const packed=brotliCompressSync(raw,{params:{[constants.BROTLI_PARAM_QUALITY]:11}});
    if(!brotliDecompressSync(packed).equals(raw))throw new Error('Archive compression verification failed; original record kept');
    this.db.prepare(`UPDATE ${table} SET payload=?,codec='br11' WHERE rowid=? AND codec IN ('plain','raw')`).run(packed,row.row_id);changed++;
   }
  }
  return changed;
 }
 stats(){
  const records=this.db.prepare("SELECT count(*) AS versions,COALESCE(SUM(raw_bytes),0) AS originalBytes,COALESCE(SUM(length(payload)),0) AS storedBytes,COALESCE(SUM(codec='br11'),0) AS compressedVersions FROM records WHERE namespace=?").get(this.namespace);
  const blobs=this.db.prepare("SELECT count(*) AS objects,COALESCE(SUM(raw_bytes),0) AS originalBytes,COALESCE(SUM(length(payload)),0) AS storedBytes,COALESCE(SUM(codec='br11'),0) AS compressedObjects FROM blobs WHERE namespace=?").get(this.namespace);
  const resources=this.db.prepare("SELECT count(*) AS files,COALESCE(SUM(raw_bytes),0) AS originalBytes,COALESCE(SUM(length(payload)),0) AS storedBytes,COALESCE(SUM(codec='br11'),0) AS compressedFiles FROM resources WHERE namespace=?").get(this.namespace);
  return {retention:'indefinite',archiveAfterDays:RETENTION_DAYS,messages:this.db.prepare('SELECT count(*) AS n FROM current_messages WHERE namespace=?').get(this.namespace).n,conversations:this.db.prepare('SELECT count(*) AS n FROM conversations WHERE namespace=?').get(this.namespace).n,records,blobs,resources,migration:this.meta('migration'),maintenance:this.meta('maintenance')};
 }
 close(){this.db.close()}
}
