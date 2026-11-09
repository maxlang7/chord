import {readFile} from 'node:fs/promises';
import path from 'node:path';
// Read saved SDK history on the HTTP thread without waiting for crypto boot.
// A new API cache can therefore display existing conversations on first use.
export class TimelineSeed {
 constructor(dataDir){this.file=path.join(dataDir,'matrix-timeline.json');this.loaded=0;this.rooms=new Map();this.messages=new Map();this.reactions=new Map()}
 async load(){
  if(this.loaded)return;
  if(!this.loading)this.loading=(async()=>{try{const d=JSON.parse(await readFile(this.file,'utf8'));this.rooms=new Map(d.chats||[]);this.messages=new Map(d.timelines||[]);this.reactions=new Map(d.reactions||[])}catch{}this.loaded=Date.now()})().finally(()=>{this.loading=null});
  await this.loading;
 }
 async page(id){
  if(!id.startsWith('matrix:'))return undefined;await this.load();const room=id.slice(7),items=this.messages.get(room);if(!items?.length)return undefined;
  return {items:items.slice(-50).map(m=>({...m,...(m.isDeleted?{text:'[Message deleted]',attachments:[],reactions:[]}:{reactions:this.reactions.get(room+'|'+m.id)||m.reactions||[]})})),hasMore:false,oldestCursor:null,newestCursor:null};
 }
 async chats(){await this.load();return [...this.rooms.values()]}
}
