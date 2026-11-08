import http from 'node:http';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {randomBytes,timingSafeEqual} from 'node:crypto';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {mailAccounts,listMail,readMail,modifyMail,sendMail,mailAttachment} from './mail-provider.mjs';
import {Directory} from './directory.mjs';
import {avatarService} from './avatars.mjs';
import {rankInbox} from './importance.mjs';
import {bridgeInfo,startBridgeLogin,submitBridgeLogin,loginState,guilds,enableGuild} from './bridge-manager.mjs';

export const root=path.dirname(fileURLToPath(import.meta.url));
const run=promisify(execFile),port=Number(process.env.PORT||4789),dataDir=process.env.CHORD_DATA_DIR||path.join(root,'private'),file=path.join(dataDir,'config.json');
process.umask(0o077);await mkdir(dataDir,{recursive:true,mode:0o700});
let config=await readFile(file,'utf8').then(JSON.parse).catch(()=>({apiKey:randomBytes(32).toString('base64url')}));
if(!config.apiKey)config.apiKey=randomBytes(32).toString('base64url');await writeFile(file,JSON.stringify(config,null,2),{mode:0o600});
const directory=await Directory.open(dataDir,{blue,config:()=>config});
const avatar=avatarService();
function equal(a,b){const x=Buffer.from(a||''),y=Buffer.from(b||'');return x.length===y.length&&timingSafeEqual(x,y)}
async function input(req){let raw='';for await(const chunk of req){raw+=chunk;if(raw.length>32*1024*1024)throw Object.assign(new Error('Request exceeds 32 MB'),{status:413})}try{return raw?JSON.parse(raw):{}}catch{throw Object.assign(new Error('Invalid JSON'),{status:400})}}
function json(res,value,status=200){res.writeHead(status,{'Content-Type':'application/json','Cache-Control':'no-store','X-Content-Type-Options':'nosniff'});res.end(JSON.stringify(value))}
export async function blue(method,signature='',args=[]){
 const {stdout}=await run('busctl',['--user','--json=short','call','io.weirdware.BlueFerry','/io/weirdware/BlueFerry','io.weirdware.BlueFerry.Messages1',method,...(signature?[signature,...args.map(String)]:[])],{timeout:30000,maxBuffer:12*1024*1024});
 const data=JSON.parse(stdout).data,value=data?.[0];if(typeof value!=='string')return value;try{return JSON.parse(value)}catch{return value}
}
export function presentThreads(threads){return threads.map(t=>({...t,confirmation_token:t.is_group?[String(t.roster_warning_id||''),...[...new Set((t.recipients||[]).map(String).filter(Boolean))].sort()].join('\n'):''}))}
export async function matrix(endpoint,method='GET',body){
 if(process.env.CHORD_TEST_TRANSPORT_URL){const r=await fetch(process.env.CHORD_TEST_TRANSPORT_URL+endpoint,{method,headers:{'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body)});const d=await r.json();if(!r.ok)throw Object.assign(new Error(d.error),{status:r.status});return d}
 if(!config.matrixAccount){if(endpoint==='/v1/accounts')return [];if(endpoint.startsWith('/v1/chats?'))return {items:[],hasMore:false};throw Object.assign(new Error('Start the local Chord runtime first.'),{status:503})}
 const {matrixRequest}=await import('./matrix-provider.mjs');return matrixRequest({config,dataDir},endpoint,method,body);
}
async function inbox(url){
 await directory.refresh();
 const view=url.searchParams.get('view')||'all';
 const source=url.searchParams.get('source')||'all',query=url.searchParams.get('query')||'',cursor=url.searchParams.get('cursor'),unread=url.searchParams.get('unread')==='true';let items=[],errors=[],cursors={};
 const jobs=[];
 if(source==='all'||['slack','discord','whatsapp','matrix'].includes(source))jobs.push((async()=>{try{const q=new URLSearchParams({limit:'200'});if(source!=='all')q.append('accountIDs',source);if(cursor)q.set('cursor',cursor);if(query)q.set('query',query);let d=await matrix('/v1/chats'+(query?'/search':'')+'?'+q);if(!cursor){const all=[...(d.items||[])];while(d.hasMore){q.set('cursor',d.oldestCursor);d=await matrix('/v1/chats'+(query?'/search':'')+'?'+q);all.push(...(d.items||[]))}d={...d,items:all}}items.push(...(d.items||[]).filter(c=>!c.accountID?.includes('blueferry')).map(c=>({...c,id:'matrix:'+c.id,remoteId:c.id,kind:'chat',source:c.accountID,title:c.title,preview:c.preview?.text||c.network||'',unread:!!(c.unreadCount||c.isMarkedUnread),timestamp:c.lastActivity,participants:(Array.isArray(c.participants)?c.participants:(c.participants?.items||[])).map(p=>typeof p==='string'?p:p.id).filter(Boolean),canReply:!c.isReadOnly,canReact:!c.isReadOnly,canSend:!c.isReadOnly})));if(d.hasMore)cursors.matrix=d.oldestCursor}catch(e){errors.push({source:'matrix',error:e.message})}})());
 if(source==='all'||source==='iphone')jobs.push((async()=>{try{const threads=presentThreads(await blue('ListThreads','u',[200]));items.push(...threads.filter(t=>!query||t.name.toLowerCase().includes(query.toLowerCase())).map(t=>({id:'blueferry:'+t.key,remoteId:t.key,kind:'phone',source:'iphone',title:t.name,preview:t.messages?.at(-1)?.body||'',unread:!!t.unread,timestamp:t.last_ts,participants:t.recipients||[],canReply:false,canReact:false,canSend:!!t.reply_ready,blue:t})))}catch(e){errors.push({source:'iphone',error:e.message})}})());
 for(const a of mailAccounts())if(source==='all'||source==='mail:'+a.id)jobs.push((async()=>{try{const d=await listMail(a.id,{query:query||'in:inbox',cursor:source==='all'?undefined:cursor,limit:source==='all'?12:35});items.push(...d.items.map(m=>({...m,id:'mail:'+a.id+':'+m.id,remoteId:m.id,kind:'email',source:'mail:'+a.id,title:m.subject,preview:m.from+' · '+m.snippet,timestamp:m.date,canReply:true,canReact:false,canSend:true})));if(d.hasMore)cursors['mail:'+a.id]=d.oldestCursor}catch(e){errors.push({source:'mail:'+a.id,error:e.message})}})());
 await Promise.all(jobs);items=items.filter(c=>(!unread||c.unread)&&!c.isArchived);const server=url.searchParams.get('server');if(server&&!/^(all|dm|[0-9]+)$/.test(server))throw Object.assign(new Error('Invalid Discord server'),{status:400});let enriched=items.map(c=>directory.enrichConversation(c));if(server)enriched=enriched.filter(c=>c.source==='discord'&&(server==='all'||(server==='dm'?!c.serverID:c.serverID===server)));const ranked=rankInbox(enriched,directory.book,{view});await directory.book.save();return {...ranked,errors,cursors,hasMore:Object.keys(cursors).length>0};
}
function parseConversation(id){if(id.startsWith('matrix:'))return {kind:'matrix',remote:id.slice(7)};if(id.startsWith('blueferry:'))return {kind:'blueferry',remote:id.slice(10)};const m=id.match(/^mail:(home|cmu):(.+)$/);if(m)return {kind:'mail',account:m[1],remote:m[2]};throw Object.assign(new Error('Unknown conversation ID'),{status:400})}
async function conversation(id,url){const c=parseConversation(id);
 if(c.kind==='matrix')return matrix('/v1/chats/'+encodeURIComponent(c.remote)+'/messages'+url.search);
 if(c.kind==='mail'){const d=await readMail(c.account,c.remote);return {...d,items:d.items.map(m=>({...m,chatID:id,senderName:m.from,timestamp:m.date,linkedMessageID:null,reactions:[]}))}}
 const t=presentThreads(await blue('ListThreads','u',[500])).find(t=>t.key===c.remote);if(!t)throw Object.assign(new Error('Phone conversation not found'),{status:404});return {items:(t.messages||[]).map((m,i)=>({...m,id:m.handle||String(i),text:m.body,timestamp:m.timestamp,isSender:m.outgoing,senderID:m.address,senderName:m.sender||m.address||t.name,reactions:[],attachments:[]})),thread:t,hasMore:false};
}
async function send(id,body){const c=parseConversation(id);if(c.kind==='matrix')return matrix('/v1/chats/'+encodeURIComponent(c.remote)+'/messages','POST',body);if(c.kind==='mail')return sendMail(c.account,{...body,threadId:c.remote});return {transfer:await blue('SendToThreadChecked','ssbs',[c.remote,body.text,!!body.confirmGroup,body.expectedGroupToken||''])}}

export const server=http.createServer(async(req,res)=>{try{
 if(!['127.0.0.1:'+port,'localhost:'+port].includes(req.headers.host))return json(res,{error:'Invalid host'},403);
 if(req.headers.origin||req.headers['sec-fetch-site']==='cross-site')return json(res,{error:'Browser cross-origin requests refused'},403);
 config=await readFile(file,'utf8').then(JSON.parse);
 if(!equal(req.headers.authorization?.replace(/^Bearer /,''),config.apiKey))return json(res,{error:'Authentication required'},401);
 const url=new URL(req.url,'http://127.0.0.1:'+port),p=url.pathname;
 if(p==='/api/status')return json(res,{version:'0.3.0',independent:true,userID:config.matrixAccount?.userId||null,matrixConfigured:!!config.matrixAccount,homeserver:config.matrixAccount?.homeserver||null,transport:'Local Matrix bridges + direct BlueFerry + Gmail',networks:(config.bridges||[]).map(({id,name})=>({id,name})),mail:mailAccounts()});
 if(p==='/api/inbox'&&req.method==='GET')return json(res,await inbox(url));
 if(p==='/api/discord/servers'&&req.method==='GET'){await directory.refresh();const rooms=[...directory.catalog.rooms.values()].filter(r=>r.network==='discord');return json(res,{items:directory.catalog.servers.map(g=>({...g,channelCount:rooms.filter(r=>r.serverID===g.id).length})).sort((a,b)=>a.name.localeCompare(b.name)),directMessageCount:rooms.filter(r=>!r.serverID).length})}
 if(p==='/api/contacts'&&req.method==='GET'){await directory.refresh();return json(res,{items:directory.book.list(url.searchParams.get('query')||''),errors:directory.catalog.errors})}
 if(p==='/api/contacts/refresh'&&req.method==='POST'){await directory.refresh(true);return json(res,{count:directory.book.list().length,errors:directory.catalog.errors})}
 if(p==='/api/contacts/merge'&&req.method==='POST'){const b=await input(req);if(b.confirm!==true)throw new Error('Review the two contacts and confirm linking');return json(res,await directory.book.merge(b.sourceID,b.targetID))}
 if(p==='/api/contacts/undo-merge'&&req.method==='POST')return json(res,await directory.book.undoMerge());
 const photo=p.match(/^\/api\/contacts\/(person_[a-f0-9]+)\/avatar$/);
 if(photo&&req.method==='GET'){await directory.refresh();const contact=directory.book.data.people[photo[1]];if(!contact||contact.redirect)return json(res,{error:'Contact not found'},404);const result=await avatar(contact,config.matrixAccount);return result?json(res,result):json(res,{error:'No profile photo available'},404)}
 const person=p.match(/^\/api\/contacts\/(person_[a-f0-9]+)$/);if(person&&req.method==='GET'){await directory.refresh();const contact=directory.book.data.people[person[1]];if(!contact||contact.redirect)return json(res,{error:'Contact not found'},404);return json(res,{...directory.book.present(contact),conversations:[...directory.conversations.values()].filter(c=>c.contactIDs.includes(contact.id)).sort((a,b)=>Date.parse(b.timestamp)-Date.parse(a.timestamp))})}if(person&&req.method==='POST')return json(res,await directory.book.update(person[1],await input(req)));
 if(p==='/api/preferences/conversation'&&req.method==='POST'){const b=await input(req);parseConversation(b.conversationID);return json(res,await directory.book.setPreference(b.conversationID,b))}

 if(p.startsWith('/api/conversations/')){
  const match=p.match(/^\/api\/conversations\/([^/]+)\/(messages|details|attachment|send|archive|read|unread|reaction|edit)$/);if(!match)return json(res,{error:'Unknown conversation action'},404);const id=decodeURIComponent(match[1]),action=match[2],c=parseConversation(id);
  if(action==='details'&&req.method==='GET'){if(c.kind==='matrix')return json(res,await matrix('/v1/chats/'+encodeURIComponent(c.remote)));if(c.kind==='blueferry')return json(res,{...(await conversation(id,url)).thread,status:await blue('GetStatus')});return json(res,{account:mailAccounts().find(a=>a.id===c.account),threadID:c.remote})}
  if(action==='messages'&&req.method==='GET'){await directory.refresh();const page=await conversation(id,url);page.items=await directory.enrichMessages(page.items||[],{source:c.kind==='mail'?'mail:'+c.account:c.kind==='blueferry'?'iphone':directory.catalog.rooms.get(c.remote)?.network});return json(res,page)}
  const b=await input(req);if(action==='send'&&req.method==='POST')return json(res,await send(id,b));
  if(action==='attachment'&&req.method==='POST'){
   if(c.kind==='blueferry')throw new Error('Bluetooth attachments are unavailable');
   if(typeof b.messageID!=='string'||!b.messageID)throw Object.assign(new Error('messageID is required'),{status:400});
   if(c.kind==='mail')return json(res,await mailAttachment(c.account,b.messageID,b.attachmentID));
   const index=b.index??0;if(!Number.isSafeInteger(index)||index<0)throw Object.assign(new Error('Attachment index must be a nonnegative integer'),{status:400});
   const {getMatrix}=await import('./matrix-provider.mjs');const provider=await getMatrix({config,dataDir});
   const message=await provider.client.messages.get({roomId:c.remote,eventId:b.messageID});
   const attachment=provider.normalizeMessage(message.message).attachments[index];
   if(!attachment)throw Object.assign(new Error('Attachment not found'),{status:404});
   if(attachment.fileSize>20*1024*1024)throw Object.assign(new Error('Attachment exceeds 20 MB'),{status:413});
   const data=await provider.download(attachment.id),bytes=Buffer.from(data.bytes);
   if(bytes.length>20*1024*1024)throw Object.assign(new Error('Attachment exceeds 20 MB'),{status:413});
   return json(res,{data:bytes.toString('base64url'),fileName:attachment.fileName,mimeType:attachment.mimeType,size:bytes.length});
  }
  if(['archive','read','unread'].includes(action)&&req.method==='POST'){
   if(c.kind==='mail')return json(res,await modifyMail(c.account,c.remote,action==='archive'?{archived:b.archived??true}:{read:action==='read'}));
   if(c.kind==='blueferry'){if(action!=='read')throw new Error('This phone action is unsupported');return json(res,{updated:await blue('MarkThreadRead','s',[c.remote])})}
   return json(res,await matrix('/v1/chats/'+encodeURIComponent(c.remote)+'/'+action,'POST',b));
  }
  if(c.kind!=='matrix')throw Object.assign(new Error('Native message replies and reactions are unavailable for this connection'),{status:422});
  if(action==='reaction')return json(res,await matrix('/v1/chats/'+encodeURIComponent(c.remote)+'/messages/'+encodeURIComponent(b.messageID)+'/reactions'+(b.remove?'/'+encodeURIComponent(b.reactionKey):''),b.remove?'DELETE':'POST',b.remove?undefined:{reactionKey:b.reactionKey}));
  if(action==='edit')return json(res,await matrix('/v1/chats/'+encodeURIComponent(c.remote)+'/messages/'+encodeURIComponent(b.messageID),'PUT',{text:b.text}));
 }
 if(p.startsWith('/api/v1/'))return json(res,await matrix(p.slice(4)+url.search,req.method,req.method==='GET'?undefined:await input(req)));
 if(p==='/api/blueferry/status')return json(res,await blue('GetStatus'));
 if(p==='/api/blueferry/threads')return json(res,presentThreads(await blue('ListThreads','u',[200])));
 if(p==='/api/blueferry/participants'&&req.method==='POST'){const b=await input(req);if(!Array.isArray(b.recipients)||b.recipients.length<2||b.recipients.length>20)throw new Error('Provide the complete verified roster of 2 to 20 recipients');return json(res,await blue('SetGroupParticipants','sas',[b.threadKey,b.recipients.length,...b.recipients]))}
 if(p==='/api/mail/accounts')return json(res,{items:mailAccounts()});
 const mail=p.match(/^\/api\/mail\/(home|cmu)\/(threads|thread|attachment|send)(?:\/([^/]+))?$/);if(mail){const [,a,action,id]=mail;if(action==='threads')return json(res,await listMail(a,{query:url.searchParams.get('query')||undefined,cursor:url.searchParams.get('cursor')}));if(action==='thread')return json(res,await readMail(a,id));if(action==='attachment')return json(res,await mailAttachment(a,url.searchParams.get('messageID'),url.searchParams.get('attachmentID')));if(action==='send'&&req.method==='POST')return json(res,await sendMail(a,await input(req)))}
 if(p==='/api/bridges')return json(res,{items:await Promise.all((config.bridges||[]).map(async b=>{try{return {id:b.id,name:b.name,available:true,info:await bridgeInfo(config,b.id)}}catch(e){return {id:b.id,name:b.name,available:false,error:e.message}}}))});
 const bridge=p.match(/^\/api\/bridges\/(whatsapp|slack|discord)\/(start|state|submit|guilds|guild)$/);if(bridge){const [,id,action]=bridge;if(action==='state')return json(res,loginState(id));if(action==='guilds')return json(res,await guilds(config));const b=await input(req);if(action==='start')return json(res,await startBridgeLogin(config,id,b.flow));if(action==='submit')return json(res,await submitBridgeLogin(config,id,b));if(action==='guild')return json(res,await enableGuild(config,b.guildID))}
 if(p==='/api/matrix/crypto'){const {getMatrix}=await import('./matrix-provider.mjs');const provider=await getMatrix({config,dataDir});return json(res,{crypto:await provider.client.crypto.status(),lastError:provider.error})}
 if(p==='/api/matrix/diagnostics'){const {getMatrix}=await import('./matrix-provider.mjs');const provider=await getMatrix({config,dataDir});const roomId=url.searchParams.get('roomID');if(!roomId)throw new Error('A roomID is required');const raw=await provider.raw('GET','/_matrix/client/v3/rooms/'+encodeURIComponent(roomId)+'/messages',undefined,{dir:'b',limit:'50'});const page=await provider.client.messages.list({roomId,limit:50,direction:'backward'});return json(res,{rawCount:raw.chunk?.length,rawTypes:(raw.chunk||[]).reduce((counts,e)=>(counts[e.type]=(counts[e.type]||0)+1,counts),{}),sdkKeys:Object.keys(page),messageCount:page.messages?.length,messageKeys:page.messages?.[0]?Object.keys(page.messages[0]):[],crypto:await provider.client.crypto.status(),lastError:provider.error})}
 if(p==='/api/matrix/refresh'&&req.method==='POST'){const {getMatrix}=await import('./matrix-provider.mjs');await (await getMatrix({config,dataDir})).refreshRooms();return json(res,{success:true})}
 if(p==='/api/openapi.json')return json(res,JSON.parse(await readFile(path.join(root,'openapi.json'),'utf8')));
 if(p==='/api/audit')return json(res,JSON.parse(await readFile(path.join(dataDir,'audit.json'),'utf8')));
 return json(res,{error:'Not found'},404);
 }catch(e){json(res,{error:e.message},e.status||500)}});
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))server.listen(port,'127.0.0.1',()=>console.log('Chord API on http://127.0.0.1:'+port));
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))for(const signal of ['SIGINT','SIGTERM'])process.once(signal,async()=>{setTimeout(()=>process.exit(0),3000).unref();try{const {closeMatrix}=await import('./matrix-provider.mjs');await closeMatrix()}finally{server.close(()=>process.exit(0))}});
