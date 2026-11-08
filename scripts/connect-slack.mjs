import {chromium} from 'playwright-core';
import {readFile,mkdir} from 'node:fs/promises';
import path from 'node:path';
const root=path.resolve(import.meta.dirname,'..'),profile=path.join(root,'private/slack-signin');
await mkdir(profile,{recursive:true,mode:0o700});
const config=JSON.parse(await readFile(path.join(root,'private/config.json'),'utf8'));
async function api(route,body){const r=await fetch('http://127.0.0.1:4789'+route,{method:'POST',headers:{Authorization:'Bearer '+config.apiKey,'Content-Type':'application/json'},body:JSON.stringify(body)});const d=await r.json();if(!r.ok)throw new Error(d.error);return d}
await api('/api/bridges/slack/start',{flow:'token'});
const context=await chromium.launchPersistentContext(profile,{executablePath:process.env.BRAVE_PATH||'/etc/profiles/per-user/maxlang/bin/brave',headless:false,viewport:null,args:['--no-first-run']});
let authToken,busy=false,complete=false;
context.on('request',request=>{try{const url=new URL(request.url());if(!url.hostname.endsWith('.slack.com')&&!['slack.com','app.slack.com'].includes(url.hostname))return;const data=request.postData()||'';let token=new URLSearchParams(data).get('token')||url.searchParams.get('token')||request.headers().authorization?.replace(/^Bearer /,'');if(!token){try{token=JSON.parse(data).token}catch{}}if(token?.startsWith('xoxc-'))authToken=token;}catch{}});
const page=context.pages()[0]||await context.newPage();await page.goto('https://app.slack.com/client/TSAU3JLKZ',{waitUntil:'domcontentloaded'});
console.log('Sign into Slack in the Brave window. Credentials are transferred only to the local Slack bridge.');
while(!complete&&context.pages().length){
 if(!authToken)for(const p of context.pages()){try{if(new URL(p.url()).hostname.endsWith('.slack.com')){authToken=await p.evaluate(()=>{const raw=localStorage.getItem('localConfig_v2');if(!raw)return null;const d=JSON.parse(raw);return Object.values(d.teams||{}).map(t=>t.token).find(t=>typeof t==='string'&&t.startsWith('xoxc-'))||null;});}}catch{}}
 const cookie=(await context.cookies('https://slack.com')).find(c=>c.name==='d');
 if(authToken&&cookie&&!busy){busy=true;try{const state=await api('/api/bridges/slack/submit',{auth_token:authToken,cookie_token:decodeURIComponent(cookie.value)});if(state.type==='complete'){complete=true;console.log('Slack connected to Chord.');await page.goto('about:blank');await page.setContent('<title>Slack connected to Chord</title><body style="background:#282828;color:#ebdbb2;font:20px system-ui;padding:80px"><h1>Slack is connected.</h1><p>Return to the Chord terminal. You can close this window.</p></body>');}}catch(e){console.error('Local Slack sign-in:',e.message);busy=false;}}
 if(!complete)await new Promise(r=>setTimeout(r,1000));
}
if(complete)await new Promise(()=>{});
