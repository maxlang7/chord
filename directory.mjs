import {ContactBook,phoneIdentity,mailbox,text,usableName} from './contacts.mjs';
import {readContactCatalog} from './contact-catalog.mjs';

export class Directory{
 constructor(book,{blue,config}){this.book=book;this.blue=blue;this.config=config;this.refreshed=0;this.catalog={rooms:new Map(),selfIds:new Set(),errors:[]};this.profiles=new Map();this.conversations=new Map()}
 static async open(dataDir,options){return new Directory(await new ContactBook(dataDir).load(),options)}
 async refresh(force=false){
  if(!force&&Date.now()-this.refreshed<60000)return;
  if(this.refreshing)return this.refreshing;
  this.refreshing=(async()=>{
   const catalog=readContactCatalog(this.book.dataDir,this.config().matrixAccount?.userId);this.catalog=catalog;
   for(const entry of catalog.identities)this.book.observe(entry);
   this.phoneRefresh=(async()=>{try{for(const p of await this.blue('ListContacts','uu',[2000,0])){
    const aliases=[...(p.phones||[]).map(phoneIdentity),...(p.emails||[]).map(e=>mailbox(e).key)].filter(Boolean);
    if(aliases.length)this.book.observe({key:aliases[0],aliases:aliases.slice(1),name:p.name,source:'iphone',rank:100,personal:true});
   }}catch(e){catalog.errors.push({source:'phone contacts',error:e.code||e.name})}await this.book.save()})().catch(()=>{});
   for(const room of catalog.rooms.values())if(room.direct&&room.otherUser){const p=this.book.person(room.otherUser),stamp=room.activity?.lastMessageAt;if(p&&stamp&&stamp>(p.lastInteractionAt||0)){p.lastInteractionAt=stamp;this.book.dirty=true}}
   this.refreshed=Date.now();await this.book.save();
  })().finally(()=>{this.refreshing=null});return this.refreshing;
 }
 observeMail(from){const entry=mailbox(from);if(!entry.key)return null;return this.book.observe({...entry,source:'email',rank:40})}
 enrichConversation(c){
  const room=this.catalog.rooms.get(c.remoteId);
  const self=this.catalog.selfIds;
  const keys=[...(room?.otherUser?[room.otherUser]:[]),...(c.participants||[])].filter(k=>!self.has(k)&&!/^@(slack|discord|whatsapp)bot:chord\.local$/.test(k));
  if(c.kind==='phone')keys.push(...(c.participants||[]).map(phoneIdentity).filter(Boolean));
  if(c.kind==='email'){
   const p=this.observeMail(c.from||'');if(p)keys.push(p.identities.find(i=>i.source==='email')?.key);
   const address=mailbox(c.from||'').key?.slice(7)||'';
   c.signals={...c.signals,automated:/^(no[._-]?reply|donotreply|notifications?|news(letter)?|marketing|updates?|digest|info|support|receipts?|billing|orders?|alerts?)\b/i.test(address.split('@')[0])||c.automated===true};
  }
  const contactIDs=[...new Set(keys.map(k=>this.book.person(k)?.id).filter(Boolean))];
  const direct=room?.direct??(c.kind==='phone'?!c.blue?.is_group:c.kind==='email'||c.direct===true);
  const other=room?.otherUser||keys.find(k=>!k.includes('bot:'));
  const resolved=this.book.name(other);
  if(direct){const stamp=room?.activity?.lastMessageAt||Date.parse(c.timestamp)||0;for(const id of contactIDs){const p=this.book.data.people[id];if(p&&stamp>(p.lastInteractionAt||0)){p.lastInteractionAt=stamp;this.book.dirty=true}}}
  const enriched={...c,previewSenderName:this.book.name(c.previewSenderID,c.previewSenderName)||c.previewSenderName||'',serverID:room?.serverID||null,serverName:room?.serverName||null,channelID:room?.portalId||null,channelName:room?.title||null,...(room?.activity?{activity:room.activity}:{}),direct,groupDM:!!room?.groupDM,contactIDs,knownContact:contactIDs.some(id=>{const p=this.book.data.people[id];return p?.customName||p?.favorite||p?.identities.some(i=>i.source!=='email')}),isBot:direct&&!!this.book.person(other)?.identities.some(i=>i.isBot),title:direct&&resolved&&c.kind!=='email'?resolved:c.title,...(room?.activity?.lastMessageAt?{timestamp:new Date(Math.max(room.activity.lastMessageAt,Date.parse(c.timestamp)||0)).toISOString()}:{})};this.conversations.set(c.id,enriched);return enriched;
 }
 async profile(key){
  if(!/^@[^:]+:chord\.local$/.test(key)||this.profiles.get(key)>Date.now())return;
  this.profiles.set(key,Date.now()+600000);
  const a=this.config().matrixAccount;if(!a)return;
  try{const r=await fetch(new URL('/_matrix/client/v3/profile/'+encodeURIComponent(key),a.homeserver),{headers:{Authorization:'Bearer '+a.accessToken},signal:AbortSignal.timeout(5000)});if(r.ok){const p=await r.json();if(p.displayname)this.book.observe({key,name:p.displayname,rank:60})}}catch{}
 }
 async enrichMessages(items,conversation){
  const source=conversation.source||conversation.accountID||'matrix';
  const keyFor=m=>source.startsWith('mail:')?mailbox(m.from||m.senderName).key:source==='iphone'?(phoneIdentity(m.senderID||m.address)||m.senderID):m.senderID;
  const unknown=[...new Set(items.filter(m=>!m.isSender&&!this.book.name(keyFor(m))).map(keyFor).filter(Boolean))];
  if(!this.profileWork)this.profileWork=(async()=>{for(let i=0;i<unknown.length;i+=4)await Promise.all(unknown.slice(i,i+4).map(k=>this.profile(k)));await this.book.save()})().catch(()=>{}).finally(()=>{this.profileWork=null});
  const mentionMap=new Map();
  for(const p of this.book.list())for(const i of p.identities){const match=i.key.match(/^@(slack|discord)_(.+):chord\.local$/);if(match){const remote=match[1]==='slack'?match[2].split('-').at(-1):match[2];const key=remote.toLowerCase();const prev=mentionMap.get(key);mentionMap.set(key,prev&&prev!==p.name?null:p.name)}}
  const result=items.map(m=>{
   const key=keyFor(m);if(source.startsWith('mail:'))this.observeMail(m.from||m.senderName);
   if(key&&!this.book.person(key))this.book.observe({key,source,name:usableName(m.senderName)?m.senderName:null});
   const p=this.book.person(key),resolved=this.book.name(key,m.senderName);
   const isSender=m.isSender||this.catalog.selfIds.has(key);
   const fallback=source==='iphone'?'Phone contact':source==='slack'?'Slack contact':source==='discord'?'Discord contact':'Unknown contact';
   return {...m,isSender,senderName:isSender?'You':resolved||fallback,senderContactID:p?.id||null,contactResolved:!!resolved,text:String(m.text||'').replace(/<@!?([a-zA-Z0-9]+)>/g,(whole,id)=>mentionMap.get(id.toLowerCase())?'@'+mentionMap.get(id.toLowerCase()):whole)};
  });await this.book.save();return result;
 }
}
