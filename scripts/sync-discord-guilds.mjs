import {readFile} from 'node:fs/promises';
const config=JSON.parse(await readFile(new URL('../private/config.json',import.meta.url),'utf8'));
const headers={Authorization:'Bearer '+config.apiKey,'Content-Type':'application/json'};
async function api(path,body){const r=await fetch('http://127.0.0.1:4789'+path,{method:body?'POST':'GET',headers,body:body?JSON.stringify(body):undefined,signal:AbortSignal.timeout(240000)});const result=await r.json();if(!r.ok)throw new Error(result.error||'Local Discord bridge failed');return result}
const {guilds}=await api('/api/bridges/discord/guilds');
const requested=new Set(process.argv.slice(2));
for(const guild of guilds){
 if(!requested.has('--all')&&!requested.has(guild.id)){console.log(`${guild.id} ${guild.name}: ${guild.bridging_mode}`);continue}
 if(guild.bridging_mode==='everything'){console.log(`${guild.name}: already connected`);continue}
 try{await api('/api/bridges/discord/guild',{guildID:guild.id});console.log(`${guild.name}: connected`)}catch(e){console.error(`${guild.name}: ${e.message}`);process.exitCode=1}
}
