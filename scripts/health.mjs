// Read-only health check. Report counts and errors without exposing messages or credentials.
import {readFile} from 'node:fs/promises';
const config=JSON.parse(await readFile(new URL('../private/config.json',import.meta.url),'utf8'));
const endpoints=process.argv.slice(2);
await Promise.all((endpoints.length?endpoints:['/api/status','/api/bridges','/api/inbox?view=all']).map(async endpoint=>{
 const started=Date.now();
 try {
  const r=await fetch('http://127.0.0.1:4789'+endpoint,{headers:{Authorization:'Bearer '+config.apiKey},signal:AbortSignal.timeout(120000)});
  const d=await r.json();
  const sources={};for(const item of d.items||[])if(item.source)sources[item.source]=(sources[item.source]||0)+1;
  console.log(JSON.stringify({endpoint,status:r.status,elapsedMs:Date.now()-started,count:d.items?.length,sources,errors:d.errors,error:d.error,...(endpoint==='/api/status'?{independent:d.independent,mail:d.mail?.map(a=>a.id),networks:d.networks?.map(a=>a.id)}:{}),...(endpoint==='/api/bridges'?{accounts:d.items}:{}),...(endpoint==='/api/blueferry/status'?{phone:d}:{})}));
 }catch(e){console.log(JSON.stringify({endpoint,error:e.message,elapsedMs:Date.now()-started}));process.exitCode=1}
}));
