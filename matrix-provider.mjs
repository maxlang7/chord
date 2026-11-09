import {mkdir} from 'node:fs/promises';
import path from 'node:path';
import {randomBytes} from 'node:crypto';
import {openMatrixStore} from './matrix-store.mjs';
import {ArchiveStore,accountNamespace,digest,DAY,RETENTION_DAYS} from './archive-store.mjs';
import {readHistory,writeHistory} from './history-file.mjs';

const enc=encodeURIComponent;
const networkFor=id=>/discord/i.test(id)?'Discord':/slack/i.test(id)?'Slack':/whatsapp/i.test(id)?'WhatsApp':/blueferry/i.test(id)?'iPhone':'Matrix';
const page=(items,limit=200,offset=0)=>({items:items.slice(offset,offset+limit),hasMore:items.length>offset+limit,oldestCursor:String(offset+limit),newestCursor:null});

export class MatrixProvider{
 constructor(client,account,{dataDir,save=true,archive}={}){this.client=client;this.account=account;this.dataDir=dataDir;this.persist=save;this.archive=archive;this.chats=new Map();this.timelines=new Map();this.reactions=new Map();this.uploads=new Map();this.mediaFiles=new Map();this.error=null;this.started=false;this.flushTimer=null;this.relationCache=new Map();this.messageFetches=new Map();this.relationRefresh=new Map();this.relationVersions=new Map();}
 async raw(method,p,body,query){
  // Metadata does not need the crypto worker. Use a bounded HTTP request so a
  // homeserver reconnect cannot leave room discovery waiting on its sync loop.
  if(this.account.homeserver){
   const url=new URL(p,this.account.homeserver);for(const [k,v] of Object.entries(query||{}))url.searchParams.set(k,v);
   const r=await fetch(url,{method,headers:{Authorization:'Bearer '+this.account.accessToken,'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(20000)});
   const value=await r.json();if(!r.ok)throw Object.assign(new Error(value.error||`Matrix returned ${r.status}`),{status:r.status});return value;
  }
  const r=await this.client.raw.request({method,path:p,body,query});if(r.status<200||r.status>=300)throw Object.assign(new Error(r.body?.error||`Matrix returned ${r.status}`),{status:r.status});return r.body
 }
 async start(){
  if(this.started)return;await this.client.boot();
  if(this.persist){const saved=await readHistory(this.dataDir).catch(e=>{if(e.code!=='ENOENT')throw e;return null});if(saved){this.chats=new Map(saved.chats||[]);this.timelines=new Map(saved.timelines||[]);this.reactions=new Map(saved.reactions||[]);this.mediaFiles=new Map(saved.mediaFiles||[])}}
  await this.refreshRooms();this.subscription=await this.client.subscribe(undefined,e=>this.onEvent(e));this.subscription.done.catch(e=>{this.error=e.message});this.started=true;
 }
 async refreshRooms(){
  if(this.roomRefresh)return this.roomRefresh;
  this.roomRefresh=this.refreshRoomsNow().finally(()=>{this.roomRefresh=null});
  return this.roomRefresh;
 }
 async refreshRoomsNow(){
  const sync=()=>this.raw('GET','/_matrix/client/v3/sync',undefined,{timeout:'0',filter:JSON.stringify({presence:{types:[]},room:{timeline:{limit:1},state:{lazy_load_members:true},ephemeral:{types:[]}}})});
  let snapshot=await sync();
  const invites=Object.entries(snapshot.rooms?.invite||{}).filter(([,room])=>room.invite_state?.events?.some(e=>e.type==='m.room.member'&&e.state_key===this.account.userId&&e.content?.membership==='invite'&&/^@(slack|discord|whatsapp)(bot|_).*:chord\.local$/.test(e.sender||'')));
  // Join in the background. An initial import may contain hundreds of rooms;
  // waiting for all of them would hide the inbox until the import completed.
  if(invites.length&&!this.inviteWork){
   this.inviteWork=(async()=>{for(let i=0;i<invites.length;i+=3)await Promise.all(invites.slice(i,i+3).map(async([roomId])=>{try{await this.raw('POST',`/_matrix/client/v3/join/${enc(roomId)}`,{})}catch(e){this.error=e.message}}))})().finally(()=>{this.inviteWork=null;this.lastRoomRefresh=0});
  }
  const dm=new Set(Object.values(snapshot.account_data?.events?.find(e=>e.type==='m.direct')?.content||{}).flat());
  for(const [roomId,room] of Object.entries(snapshot.rooms?.join||{})){
   const state=[...(room.state?.events||[]),...(room.timeline?.events||[]).filter(e=>e.state_key!==undefined)];const find=t=>state.find(e=>e.type===t)?.content;
   if(find('m.room.create')?.type==='m.space')continue;
   const bridge=find('m.bridge')||find('uk.half-shot.bridge');let accountID=bridge?.protocol?.id||'matrix';accountID=/slack/i.test(accountID)?'slack':/discord/i.test(accountID)?'discord':/whatsapp/i.test(accountID)?'whatsapp':accountID;
   const people=state.filter(e=>e.type==='m.room.member'&&e.content?.membership==='join').map(e=>({id:e.state_key,fullName:e.content.displayname||e.state_key,isSelf:e.state_key===this.account.userId}));
   const old=this.chats.get(roomId)||{};const prefs=room.account_data?.events?.find(e=>e.type==='org.chord.chat')?.content||{};const marked=room.account_data?.events?.find(e=>e.type==='m.marked_unread')?.content?.unread;
   const latest=room.timeline?.events?.at(-1);const title=find('m.room.name')?.name||bridge?.channel?.displayname||people.filter(p=>!p.isSelf).map(p=>p.fullName).slice(0,3).join(', ')||roomId;
   const locallyRead=old.localReadAt&&Number(latest?.origin_server_ts||Date.parse(old.lastActivity)||0)<=old.localReadAt;
   const rawHighlights=room.unread_notifications?.highlight_count||0;
   const readHighlightBaseline=old.localReadAt?Math.min(old.readHighlightBaseline??rawHighlights,rawHighlights):0;
   this.chats.set(roomId,{...old,id:roomId,accountID,network:bridge?.protocol?.displayname||networkFor(accountID),title,description:find('m.room.topic')?.topic,type:dm.has(roomId)||room.summary?.['m.joined_member_count']===2?'single':'group',participants:{items:people.length?people:old.participants?.items||[],total:room.summary?.['m.joined_member_count']||people.length,hasMore:(room.summary?.['m.joined_member_count']||people.length)>people.length},lastActivity:new Date(latest?.origin_server_ts||Date.parse(old.lastActivity)||0).toISOString(),rawHighlightCount:rawHighlights,readHighlightBaseline,highlightCount:locallyRead?0:Math.max(0,rawHighlights-readHighlightBaseline),unreadCount:locallyRead?0:room.unread_notifications?.notification_count||0,isMarkedUnread:locallyRead?false:marked??old.isMarkedUnread??false,isArchived:prefs.archived??old.isArchived??false,isMuted:old.isMuted||false,isReadOnly:false,capabilities:{reply:2,reaction:2,edit:2,archive:true,markAsUnread:true},preview:latest?.content?.body?{text:latest.content.body}:old.preview});
  }
  this.scheduleSave();
  this.lastRoomRefresh=Date.now();
 }
 scheduleSave(){if(!this.persist)return;clearTimeout(this.flushTimer);this.flushTimer=setTimeout(()=>{this.save().catch(e=>this.error=e.message)},300)}
 async save(){
  if(!this.persist)return;
  this.coldSaved??=new Map();
  const cutoff=Date.now()-RETENTION_DAYS*DAY,timelines=[],hotIDs=new Set();
  for(const [id,messages] of this.timelines){
   const hot=[],pending=[];
   for(const m of messages){
    if(!this.archive||Date.parse(m.timestamp)>=cutoff||!Number.isFinite(Date.parse(m.timestamp))){hot.push(m);hotIDs.add(id+'|'+m.id);continue}
    const saved={...m,reactions:this.reactions.get(id+'|'+m.id)||m.reactions||[]},hash=digest(JSON.stringify(saved)),key=id+'|'+m.id;
    // Confirm the full old version is durably captured before omitting its body
    // from the small startup snapshot. A failed capture leaves the old file intact.
    if(this.coldSaved.get(key)!==hash)pending.push({saved,key,hash});
   }
   for(let i=0;i<pending.length;i+=100){const batch=pending.slice(i,i+100);this.retain(id,batch.map(x=>x.saved),{historical:true});for(const x of batch)this.coldSaved.set(x.key,x.hash)}
   timelines.push([id,hot]);
  }
  const snapshot={chats:[...this.chats],timelines,reactions:[...this.reactions].filter(([key])=>hotIDs.has(key)),mediaFiles:[...this.mediaFiles]};
  // Snapshot eagerly so queued writes cannot observe a subsequently mutated message.
  const stable=JSON.parse(JSON.stringify(snapshot));
  this.saveWork=(this.saveWork||Promise.resolve()).catch(()=>{}).then(()=>writeHistory(this.dataDir,stable));
  return this.saveWork;
 }

 onEvent(event){
  if(event.kind==='invite'&&event.roomId&&/^@(slack|discord|whatsapp)(bot|_).*:chord\.local$/.test(event.inviter||'')){this.client.rooms.join({roomIdOrAlias:event.roomId}).then(()=>this.refreshRooms()).catch(e=>this.error=e.message);return}
  if(event.kind==='error'||event.kind==='decryptionError'){this.error=event.error;return}
  const room=this.chats.get(event.roomId);
  if(event.kind==='message'){
   const message=this.normalizeMessage(event);const timeline=this.timelines.get(event.roomId)||[];const index=timeline.findIndex(m=>m.id===message.id);if(index>=0)timeline[index]=message;else timeline.push(message);timeline.sort((a,b)=>new Date(a.timestamp)-new Date(b.timestamp));this.timelines.set(event.roomId,timeline);
   if(room){room.preview=message;room.lastActivity=message.timestamp;}
  }else if(event.kind==='reaction'){
   const key=event.roomId+'|'+event.relatesTo;this.relationVersions.set(key,(this.relationVersions.get(key)||0)+1);const current=this.reactions.get(key)||this.archive?.message('matrix:'+event.roomId,event.relatesTo)?.reactions||[];const filtered=current.filter(r=>r.id!==event.eventId);if(event.added)filtered.push({id:event.eventId,reactionKey:event.key,participantID:event.sender.userId});this.reactions.set(key,filtered);
  }else if(event.kind==='event'&&event.type==='m.room.redaction'){
   const redacts=event.raw?.redacts||event.content?.redacts;for(const [key,items] of this.reactions)this.reactions.set(key,items.filter(r=>r.id!==redacts));const m=this.timelines.get(event.roomId)?.find(m=>m.id===redacts)||this.archive?.message('matrix:'+event.roomId,redacts);if(m){m.isDeleted=true;m.text='[Message deleted]';m.attachments=[];m.reactions=[];this.retain(event.roomId,[m])}
  }else if(event.class==='state'&&room){if(event.type==='m.room.name')room.title=event.content?.name||room.title;if(event.type==='m.room.topic')room.description=event.content?.topic;}
  if(event.kind==='message')this.retain(event.roomId,[this.timelines.get(event.roomId).find(m=>m.id===event.eventId)]);
  else if(event.kind==='reaction'){const m=this.timelines.get(event.roomId)?.find(m=>m.id===event.relatesTo)||this.archive?.message('matrix:'+event.roomId,event.relatesTo);if(m)this.retain(event.roomId,[m])}
  this.scheduleSave();
 }
 retain(roomId,messages,options){const c=this.chats.get(roomId)||{};this.archive?.recordMessages('matrix:'+roomId,messages.filter(Boolean).map(m=>({...m,reactions:this.reactions.get(roomId+'|'+m.id)||m.reactions||[]})),{title:c.title||roomId,source:c.accountID||'matrix',kind:'chat',timestamp:c.lastActivity},options)}
 normalizeMessage(m){
  const room=this.chats.get(m.roomId),userId=m.sender?.userId||m.raw?.sender;const person=room?.participants?.items.find(p=>p.id===userId);const attachments=(m.attachments||[]).map(a=>{
   const id=a.contentUri||a.encryptedFile?.url;if(id&&a.encryptedFile)this.mediaFiles.set(id,a.encryptedFile);
   return {id,type:a.kind==='image'?'img':a.kind,fileName:a.filename,mimeType:a.contentType,fileSize:a.size,...(a.width?{size:{width:a.width,height:a.height}}:{})};
  });
  return {id:m.eventId,chatID:m.roomId,accountID:room?.accountID||'matrix',senderID:userId,senderName:person?.fullName||userId,timestamp:new Date(m.timestamp||m.raw?.origin_server_ts||0).toISOString(),sortKey:String(m.timestamp||0),text:m.text||'',isMention:!!m.mentions?.userIds?.includes(this.account.userId),isSender:m.sender?.isMe??userId===this.account.userId,linkedMessageID:m.replyTo||m.threadRoot,attachments,isDeleted:false,...(m.edited?{editedTimestamp:new Date(m.timestamp||0).toISOString()}:{}),reactions:this.reactions.get(m.roomId+'|'+m.eventId)||[]};
 }
 async getChat(roomId){
  let room=this.chats.get(roomId);if(!room){await this.refreshRooms();room=this.chats.get(roomId)}if(!room){const info=await this.client.rooms.get({roomId});room={id:roomId,accountID:'matrix',network:'Matrix',title:info.name||roomId,description:info.topic,type:info.isDM?'single':'group',participants:{items:[],total:info.memberCount||0,hasMore:false},lastActivity:new Date().toISOString(),unreadCount:0,isReadOnly:false,capabilities:{reply:2,reaction:2,edit:2,archive:true,markAsUnread:true}};this.chats.set(roomId,room);}
  const {members}=await this.client.rooms.listMembers({roomId,membership:'join'});room.participants={items:members.map(m=>({id:m.userId,fullName:m.displayName||m.userId,isSelf:m.userId===this.account.userId})),total:members.length,hasMore:false};return room;
 }
 async refreshReactions(roomId,items){
  if(this.relationRefresh.has(roomId))return this.relationRefresh.get(roomId);
  const due=items.filter(m=>Date.now()-(this.relationCache.get(roomId+'|'+m.id)||0)>300000);
  if(!due.length)return;
  const work=(async()=>{
   for(let offset=0;offset<due.length;offset+=6)await Promise.all(due.slice(offset,offset+6).map(async m=>{
    const key=roomId+'|'+m.id,version=this.relationVersions.get(roomId+'|'+m.id)||0;
    try{const data=await this.raw('GET',`/_matrix/client/v1/rooms/${enc(roomId)}/relations/${enc(m.id)}/m.annotation/m.reaction`,undefined,{limit:'100'});if(version!==(this.relationVersions.get(key)||0))return;this.reactions.set(key,(data.chunk||[]).map(e=>({id:e.event_id,reactionKey:e.content?.['m.relates_to']?.key,participantID:e.sender})).filter(r=>r.reactionKey));this.relationCache.set(key,Date.now())}catch{}
   }));
   this.retain(roomId,items);this.scheduleSave();
  })().finally(()=>{this.relationRefresh.delete(roomId)});
  this.relationRefresh.set(roomId,work);return work;
 }
 async listMessages(roomId,cursor,direction){
  const key=JSON.stringify([roomId,cursor||'',direction||'']);
  if(this.messageFetches.has(key))return this.messageFetches.get(key);
  const work=this.fetchMessages(roomId,cursor,direction).finally(()=>{this.messageFetches.delete(key)});
  this.messageFetches.set(key,work);return work;
 }
 async fetchMessages(roomId,cursor,direction){
  if(!this.chats.has(roomId))await this.getChat(roomId);
  const result=await this.client.messages.list({roomId,limit:50,...(cursor?{cursor}:{}),direction:direction==='after'?'forward':'backward'});
  const items=result.messages.map(m=>this.normalizeMessage(m));
  // Cached emoji render with the text; recovering older reactions never blocks it.
  this.refreshReactions(roomId,items).catch(e=>{this.error=e.message});
  const prior=this.timelines.get(roomId)||[];
  this.timelines.set(roomId,[...new Map([...prior,...items].map(m=>[m.id,m])).values()].sort((a,b)=>new Date(a.timestamp)-new Date(b.timestamp)));
  this.retain(roomId,items);
  this.scheduleSave();
  return {items,hasMore:!!result.nextCursor,oldestCursor:result.nextCursor||null,newestCursor:null};
 }

 async download(url){if(this.mediaFiles.has(url))return this.client.media.downloadEncrypted({file:this.mediaFiles.get(url)});return this.client.media.download({contentUri:url})}
 async request(endpoint,method='GET',body={}){
  const u=new URL(endpoint,'http://chord.local'),p=u.pathname,q=u.searchParams;
  if(p==='/v1/accounts')return [...new Map([...this.chats.values()].map(c=>[c.accountID,{accountID:c.accountID,network:c.network}])).values()];
  if(p==='/v1/chats'||p==='/v1/chats/search'){
   if(method==='POST'){const result=await this.client.rooms.create({invite:body.participantIDs,isDirect:body.type==='single',name:body.title,preset:'private_chat',initialState:[{type:'m.room.encryption',stateKey:'',content:{algorithm:'m.megolm.v1.aes-sha2'}}]});await this.refreshRooms();if(body.messageText)await this.client.messages.send({roomId:result.roomId,text:body.messageText});return this.getChat(result.roomId)}
   if(Date.now()-(this.lastRoomRefresh||0)>5000)await this.refreshRooms();
   let rooms=[...this.chats.values()].filter(c=>!q.has('accountIDs')||q.getAll('accountIDs').includes(c.accountID));
   if(q.get('query'))rooms=rooms.filter(c=>c.title.toLowerCase().includes(q.get('query').toLowerCase()));if(q.get('type')&&q.get('type')!=='any')rooms=rooms.filter(c=>c.type===q.get('type'));if(q.get('unreadOnly')==='true')rooms=rooms.filter(c=>c.unreadCount||c.isMarkedUnread);if(q.get('inbox'))rooms=rooms.filter(c=>q.get('inbox')==='archive'?c.isArchived:!c.isArchived);
   rooms=rooms.map(c=>{const timeline=this.timelines.get(c.id)||[];const own=new Set(timeline.filter(m=>m.isSender).map(m=>m.id));return {...c,direct:c.type==='single',signals:{mention:!!c.highlightCount||timeline.some(m=>!m.isSender&&m.isMention&&Date.parse(m.timestamp)>Date.now()-30*86400000),replyToYou:timeline.some(m=>!m.isSender&&own.has(m.linkedMessageID)&&Date.parse(m.timestamp)>Date.now()-30*86400000)},preview:timeline.at(-1)?.text?{text:timeline.at(-1).text}:c.preview}});
   rooms.sort((a,b)=>new Date(b.lastActivity)-new Date(a.lastActivity));return page(rooms,Math.min(200,Number(q.get('limit'))||200),Number(q.get('cursor'))||0);
  }
  if(p==='/v1/messages/search'){
   const text=(q.get('query')||'').toLowerCase(),items=[...this.timelines.values()].flat().filter(m=>m.text?.toLowerCase().includes(text)&&(!q.has('accountIDs')||q.getAll('accountIDs').includes(m.accountID))).sort((a,b)=>new Date(b.timestamp)-new Date(a.timestamp));return {...page(items.map(m=>({...m,conversation:this.chats.get(m.chatID)})),Math.min(20,Number(q.get('limit'))||20),Number(q.get('cursor'))||0),scope:'Messages synced or fetched by Chord. Encrypted history search expands as history is loaded.'};
  }
  if(p==='/v1/assets/upload/base64'&&method==='POST'){
   const bytes=Buffer.from(body.content,'base64');if(bytes.length>20*1024*1024)throw new Error('Attachments must be at most 20 MB');const id=randomBytes(16).toString('hex');this.uploads.set(id,{bytes,name:body.fileName,mime:body.mimeType,at:Date.now()});for(const [k,v] of this.uploads)if(Date.now()-v.at>3600000)this.uploads.delete(k);return {uploadID:id,fileName:body.fileName,mimeType:body.mimeType,fileSize:bytes.length};
  }
  if(p==='/v1/chats/start'&&method==='POST'){if(!body.user?.id)throw new Error('Use a complete Matrix user ID for direct Matrix chats');const r=await this.client.rooms.openDM({userId:body.user.id});await this.refreshRooms();return this.getChat(r.roomId)}
  const match=p.match(/^\/v1\/chats\/([^/]+)(?:\/messages(?:\/([^/]+)(?:\/reactions(?:\/([^/]+))?)?)?|\/(archive|read|unread|reminders))?$/);
  if(match){
   const roomId=decodeURIComponent(match[1]),eventId=match[2]&&decodeURIComponent(match[2]),key=match[3]&&decodeURIComponent(match[3]),action=match[4];
   if(action){const c=this.chats.get(roomId)||await this.getChat(roomId);
    if(action==='archive'){c.isArchived=body.archived??true;await this.client.accountData.setRoom({roomId,eventType:'org.chord.chat',content:{archived:c.isArchived}})}
    else if(action==='unread'||action==='read'){
     const unread=action==='unread';let id=body.messageID;
     // Unopened rooms may have no local timeline, or only older fetched history.
     // Advance the receipt to the current room head rather than clearing only a badge.
     if(!unread&&!id){const page=await this.raw('GET',`/_matrix/client/v3/rooms/${enc(roomId)}/messages`,undefined,{dir:'b',limit:'1'});id=page.chunk?.[0]?.event_id}
     await this.client.accountData.setRoom({roomId,eventType:'m.marked_unread',content:{unread}});
     if(!unread&&id)await this.client.messages.markRead({roomId,eventId:id});
     c.unreadCount=unread?1:0;c.isMarkedUnread=unread;c.localReadAt=unread?0:Date.now();c.readHighlightBaseline=unread?0:c.rawHighlightCount??c.highlightCount??0;if(!unread)c.highlightCount=0;
    }
    else throw Object.assign(new Error('Reminders are not implemented by the independent transport'),{status:501});this.scheduleSave();return c;
   }
   if(p.includes('/reactions')){
    if(method==='POST'){const r=await this.client.reactions.send({roomId,eventId,key:body.reactionKey});const k=roomId+'|'+eventId;this.relationVersions.set(k,(this.relationVersions.get(k)||0)+1);this.reactions.set(k,[...(this.reactions.get(k)||[]).filter(x=>x.id!==r.eventId),{id:r.eventId,reactionKey:body.reactionKey,participantID:this.account.userId}]);this.relationCache.set(k,Date.now());this.scheduleSave();return {success:true,messageID:eventId,chatID:roomId,reactionKey:body.reactionKey,eventID:r.eventId}}
    if(method==='DELETE'){await this.client.reactions.redact({roomId,eventId,key});const k=roomId+'|'+eventId;this.relationVersions.set(k,(this.relationVersions.get(k)||0)+1);this.reactions.set(k,(this.reactions.get(k)||[]).filter(x=>!(x.reactionKey===key&&x.participantID===this.account.userId)));this.relationCache.set(k,Date.now());this.scheduleSave();return {success:true}}
   }
   if(eventId){
    if(method==='GET'){const {message}=await this.client.messages.get({roomId,eventId});if(!message)throw Object.assign(new Error('Message not found'),{status:404});return this.normalizeMessage(message)}
    if(method==='PUT'){const r=await this.client.messages.edit({roomId,eventId,text:body.text});return {id:r.eventId,chatID:roomId,text:body.text}}
    if(method==='DELETE'){await this.client.messages.redact({roomId,eventId});return {success:true}}
   }
   if(p.endsWith('/messages')){
    if(method==='GET')return this.listMessages(roomId,q.get('cursor'),q.get('direction'));
    if(method==='POST'){
     let r;if(body.attachment){const upload=this.uploads.get(body.attachment.uploadID);if(!upload)throw new Error('Attachment upload expired');if(body.replyToMessageID)throw new Error('Replies with attachments are not yet supported by the independent transport. Send the text reply or attachment separately.');r=await this.client.messages.sendMedia({roomId,bytes:upload.bytes,filename:upload.name,contentType:upload.mime,caption:body.text,kind:upload.mime?.startsWith('image/')?'image':upload.mime?.startsWith('video/')?'video':upload.mime?.startsWith('audio/')?'audio':'file'});this.uploads.delete(body.attachment.uploadID)}else{if(!body.text?.trim())throw new Error('Message is empty');r=await this.client.messages.send({roomId,text:body.text,replyTo:body.replyToMessageID})}return {chatID:roomId,pendingMessageID:r.eventId};
    }
   }
   if(method==='GET')return this.getChat(roomId);
  }
  throw Object.assign(new Error('This endpoint is not supported by the independent Matrix transport'),{status:501});
 }
}
let instance,boot;
export async function getMatrix({config,dataDir}){
 if(instance)return instance;
 if(boot)return boot;
 boot=(async()=>{
  const {createMatrixClient}=await import('./vendor/pickle/packages/pickle/dist/node.js');
  const store=await openMatrixStore(dataDir);
  await mkdir(path.join(dataDir,'matrix-crypto'),{recursive:true,mode:0o700});
  const client=createMatrixClient({account:config.matrixAccount,store,pickleKey:config.pickleKey,recoveryKey:config.recoveryKey,logger:(level,message)=>{if(level==='error')console.error('Matrix:',message)}});
  const archive=new ArchiveStore(path.join(dataDir,'archive.sqlite'),{namespace:accountNamespace(config)});
  const provider=new MatrixProvider(client,config.matrixAccount,{dataDir,archive});await provider.start().catch(async e=>{await client.close().catch(()=>{});store.close();archive.close();throw e});provider.store=store;instance=provider;return provider;
 })().catch(e=>{boot=null;throw e});return boot;
}
export async function matrixRequest(context,endpoint,method,body){return (await getMatrix(context)).request(endpoint,method,body)}
export async function closeMatrix(){const provider=instance||await boot?.catch(()=>null);if(!provider)return;clearTimeout(provider.flushTimer);if(provider.subscription)await provider.subscription.stop();await provider.client.close();clearTimeout(provider.flushTimer);await provider.save();provider.store?.close();provider.archive?.close();instance=null;boot=null;}
