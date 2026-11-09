// Read-only timing of inbox and conversation reads. Never logs message bodies.
import {readFile,writeFile} from 'node:fs/promises';
const config=JSON.parse(await readFile(new URL('../private/config.json',import.meta.url),'utf8'));
async function api(route){const start=performance.now();const r=await fetch('http://127.0.0.1:4789'+route,{headers:{Authorization:'Bearer '+config.apiKey},signal:AbortSignal.timeout(120000)});const d=await r.json();if(!r.ok)throw new Error(d.error);return {d,ms:Math.round((performance.now()-start)*10)/10}}
const report={checkedAt:new Date().toISOString(),inbox:[],messages:[]};
for(const source of ['whatsapp','slack','discord','mail:home','mail:cmu','iphone']){
 let inbox;
 for(let n=0;n<2;n++){const r=await api('/api/inbox?source='+encodeURIComponent(source)+'&view=all');inbox=r.d;report.inbox.push({source,pass:n+1,ms:r.ms,count:inbox.items.length,cached:!!inbox._cache})}
 for(const [index,c] of inbox.items.slice(0,2).entries())for(let n=0;n<2;n++){
  const r=await api('/api/conversations/'+encodeURIComponent(c.id)+'/messages');report.messages.push({source,thread:index+1,pass:n+1,ms:r.ms,count:r.d.items.length,cached:!!r.d._cache});
 }
}
await writeFile(new URL('../private/cache-benchmark.json',import.meta.url),JSON.stringify(report,null,2),{mode:0o600});
console.log(JSON.stringify(report,null,2));
