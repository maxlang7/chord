// Bounded, read-only checks against existing conversations. No messages are sent,
// marked read, archived, or reacted to. Message bodies are never logged.
import {readFile,writeFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
const config=JSON.parse(await readFile(new URL('../private/config.json',import.meta.url),'utf8'));
async function api(route){const r=await fetch('http://127.0.0.1:4789'+route,{headers:{Authorization:'Bearer '+config.apiKey},signal:AbortSignal.timeout(120000)});const d=await r.json();if(!r.ok)throw new Error(d.error||String(r.status));return d}
const status=await api('/api/status');assert.equal(status.independent,true);
const connections=await api('/api/connections?refresh=true');
const inbox=await api('/api/inbox?view=all');assert.ok(inbox.items.length>0);assert.equal(inbox.errors.length,0);
const results={checkedAt:new Date().toISOString(),outboundMessages:0,totalConversations:inbox.items.length,connections:connections.items.map(({id,connected,state})=>({id,connected,state})),sources:{},messages:{},errors:[]};
for(const c of inbox.items)results.sources[c.source]=(results.sources[c.source]||0)+1;
await Promise.all(Object.keys(results.sources).map(async source=>{
 const c=inbox.items.find(c=>c.source===source&&c.kind!=='phone')||inbox.items.find(c=>c.source===source);
 try{const page=await api('/api/conversations/'+encodeURIComponent(c.id)+'/messages');results.messages[source]={loaded:page.items?.length||0,attachments:(page.items||[]).reduce((n,m)=>n+(m.attachments?.length||0),0),reactions:(page.items||[]).reduce((n,m)=>n+(m.reactions?.length||0),0)};
 }catch(e){results.errors.push({source,error:e.message})}
}));
const search=await api('/api/search?query='+encodeURIComponent('meeting'));results.search={hits:search.items.length,errors:search.errors};
await writeFile(new URL('../private/live-verification.json',import.meta.url),JSON.stringify(results,null,2),{mode:0o600});
console.log(JSON.stringify(results,null,2));
if(results.errors.length)process.exitCode=1;
