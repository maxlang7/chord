import {readFile,writeFile,rename,mkdir} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import path from 'node:path';

export const text=s=>String(s||'').replace(/[\x00-\x1f\x7f-\x9f]/g,'').trim();
export function emailIdentity(value){const match=String(value||'').match(/<?([^\s<>]+@[^\s<>]+)>?/);return match?'mailto:'+match[1].toLowerCase():null}
export function phoneIdentity(value){let phone=String(value||'').replace(/[\s().-]/g,'');if(phone.startsWith('00'))phone='+'+phone.slice(2);return /^\+[1-9]\d{6,14}$/.test(phone)?'tel:'+phone:null}
export function mailbox(value){const key=emailIdentity(value);const label=text(String(value||'').replace(/<[^>]+>/,'').replace(/^"|"$/g,''));return {key,name:label&&label.toLowerCase()!==key?.slice(7)?label:null}}
function identity(value){if(!value)return null;if(value.startsWith('tel:'))return phoneIdentity(value.slice(4));if(value.startsWith('mailto:'))return emailIdentity(value.slice(7));return text(value)}
export function usableName(name){name=text(name);return !!name&&!/^@[^\s]+:[^\s]+$/.test(name)&&!/^\+?[\d\s().-]{7,}$/.test(name)&&!/^\d{12,}$/.test(name)}
const idFor=key=>'person_'+createHash('sha256').update(key).digest('hex').slice(0,20);

export class ContactBook{
 constructor(dataDir){this.dataDir=dataDir;this.file=path.join(dataDir,'contacts.json');this.data={version:1,people:{},links:{},conversations:{}};this.writes=Promise.resolve();this.dirty=false;}
 async load(){try{const data=JSON.parse(await readFile(this.file,'utf8'));if(data.version!==1||!data.people||!data.links)throw new Error('Unsupported contact database');this.data={...this.data,...data}}catch(e){if(e.code!=='ENOENT')throw e}return this}
 person(key){return this.data.people[this.data.links[identity(key)]]}
 observe({key,name,source='matrix',aliases=[],isBot,handle,label,personal,rank=50}){
  const keys=[...new Set([key,...aliases].map(identity).filter(Boolean))];if(!keys.length)return null;
  // Link only an exact email/phone identity supplied by a source. Similar names
  // never imply that two accounts belong to the same person.
  let person=keys.map(k=>this.person(k)).find(Boolean);
  if(!person){const id=idFor(keys[0]);person={id,identities:[],favorite:false,createdAt:new Date().toISOString()};this.data.people[id]=person;this.dirty=true}
  for(const k of keys){
   const existing=this.person(k);
   if(existing&&existing.id!==person.id){
    // Keep explicit user links and conflicting renamed records separate.
    if(existing.customName||person.customName)continue;
    this.mergeInto(person,existing);
   }
   if(this.data.links[k]!==person.id){this.data.links[k]=person.id;this.dirty=true}
   let entry=person.identities.find(i=>i.key===k);if(!entry){entry={key:k,source:k.startsWith('tel:')?'phone':k.startsWith('mailto:')?'email':source};person.identities.push(entry);this.dirty=true}
   if(usableName(name)&&(!entry.name||rank>=(entry.rank||0))&&entry.name!==text(name)){entry.name=text(name);entry.rank=rank;this.dirty=true}
   if(isBot!==undefined&&entry.isBot!==isBot){entry.isBot=isBot;this.dirty=true}if(handle&&entry.handle!==handle){entry.handle=text(handle);this.dirty=true}
   if(label&&entry.label!==label){entry.label=text(label);this.dirty=true}
   if(personal&&!entry.personal){entry.personal=true;this.dirty=true}
  }
  return person;
 }
 mergeInto(target,other){
  for(const entry of other.identities){if(!target.identities.some(i=>i.key===entry.key))target.identities.push(entry);this.data.links[entry.key]=target.id}
  target.favorite=!!(target.favorite||other.favorite);target.customName||=other.customName;
  target.lastInteractionAt=Math.max(target.lastInteractionAt||0,other.lastInteractionAt||0);
  // A redirect preserves saved links and makes an accidental merge reversible.
  other.redirect=target.id;other.identities=[];this.dirty=true;
 }
 name(key,fallback){const p=this.person(key);if(p){if(p.customName)return p.customName;const entry=p.identities.filter(i=>usableName(i.name)).sort((a,b)=>(b.rank||0)-(a.rank||0))[0];if(entry)return entry.name}return usableName(fallback)?text(fallback):null}
 present(person){const name=person.customName||this.name(person.identities[0]?.key)||person.identities.find(i=>i.handle)?.handle||person.identities[0]?.key.replace(/^mailto:|^tel:/,'')||'Unknown contact';return {...person,name,unresolved:!person.customName&&!person.identities.some(i=>usableName(i.name)),isBot:person.identities.some(i=>i.isBot)}}
 list(query=''){const q=query.toLowerCase();return Object.values(this.data.people).filter(p=>!p.redirect).map(p=>this.present(p)).filter(p=>!q||[p.name,...p.identities.map(i=>i.key),...p.identities.map(i=>i.name||'')].join(' ').toLowerCase().includes(q)).sort((a,b)=>Number(b.favorite)-Number(a.favorite)||Number(!!b.lastInteractionAt||b.identities.some(i=>i.personal))-Number(!!a.lastInteractionAt||a.identities.some(i=>i.personal))||(b.lastInteractionAt||0)-(a.lastInteractionAt||0)||a.name.localeCompare(b.name))}
 async update(id,changes){const p=this.data.people[id];if(!p||p.redirect)throw Object.assign(new Error('Contact not found'),{status:404});if(Object.hasOwn(changes,'name')){const name=text(changes.name);if(!name||name.length>120)throw new Error('Use a contact name between 1 and 120 characters');p.customName=name}if(typeof changes.favorite==='boolean')p.favorite=changes.favorite;this.dirty=true;await this.save();return this.present(p)}
 async merge(sourceID,targetID){const source=this.data.people[sourceID],target=this.data.people[targetID];if(!source||!target||source.redirect||target.redirect||source===target)throw new Error('Choose two different existing contacts');const before=structuredClone({source,target});this.mergeInto(target,source);this.data.lastMerge={...before,mergedName:target.customName||null,mergedFavorite:target.favorite,at:new Date().toISOString()};await this.save();return this.present(target)}
 async undoMerge(){const previous=this.data.lastMerge;if(!previous)throw new Error('No contact link to undo');const current=this.data.people[previous.target.id];if((current.customName||null)!==previous.mergedName||current.favorite!==previous.mergedFavorite||current.identities.some(i=>!previous.target.identities.some(j=>j.key===i.key)&&!previous.source.identities.some(j=>j.key===i.key)))throw new Error('Contacts changed since linking; review the current links before undoing');for(const p of [previous.source,previous.target]){this.data.people[p.id]=p;for(const i of p.identities)this.data.links[i.key]=p.id}delete this.data.lastMerge;this.dirty=true;await this.save();return {success:true}}
 preference(id){return this.data.conversations[id]||{priority:'auto'}}
 async setPreference(id,{priority}){if(!['auto','important','quiet'].includes(priority))throw new Error('Priority must be auto, important, or quiet');this.data.conversations[id]={priority};this.dirty=true;await this.save();return this.preference(id)}
 async save(){if(!this.dirty)return this.writes;this.dirty=false;const json=JSON.stringify(this.data);this.writes=this.writes.catch(()=>{}).then(async()=>{await mkdir(this.dataDir,{recursive:true,mode:0o700});const temp=this.file+'.pending';await writeFile(temp,json,{mode:0o600});await rename(temp,this.file)});return this.writes}
}
