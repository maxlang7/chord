import WebSocket from 'ws';
const flows=new Map();
function bridge(config,id){const b=config.bridges?.find(b=>b.id===id);if(!b)throw Object.assign(new Error('This bridge has not been configured'),{status:503});return b}
async function call(config,id,endpoint,method='GET',body,timeout=90000){const b=bridge(config,id),url=new URL('/_matrix/provision'+endpoint,b.url);url.searchParams.set('user_id',config.matrixAccount.userId);const r=await fetch(url,{method,headers:{Authorization:'Bearer '+b.secret,'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(timeout)});const d=await r.json();if(!r.ok)throw Object.assign(new Error(d.error||d.message||'Bridge request failed'),{status:r.status});return d}
export async function bridgeInfo(config,id){const b=bridge(config,id);return call(config,id,b.version===1?'/v1/ping':'/v3/whoami','GET',undefined,8000)}
export function loginState(id){return flows.get(id)?.state||{type:'idle',instructions:'Choose Connect to sign in.'}}
async function accept(config,id,state){const old=flows.get(id)||{};flows.set(id,{...old,state});if(state.type==='display_and_wait'){
 const txn=state.txn_id?'?txn_id='+encodeURIComponent(state.txn_id):'';
 call(config,id,`/v3/login/step/${encodeURIComponent(state.login_id)}/${encodeURIComponent(state.step_id)}/display_and_wait${txn}`,'POST',{}).then(next=>accept(config,id,next)).catch(e=>{const current=flows.get(id);if(current?.state===state)current.state={type:'error',instructions:e.message}});
}return state}
export async function startBridgeLogin(config,id,flow){
 const b=bridge(config,id);if(flows.get(id)?.state?.type==='display_and_wait')return loginState(id);
 if(b.version===1){
  if(flow==='token'){const state={type:'user_input',instructions:'Paste your Discord user token locally. It is sent only to your local bridge.',user_input:{fields:[{id:'token',name:'Discord user token',type:'token'}]}};flows.set(id,{state});return state}
  const state={type:'display_and_wait',instructions:'Preparing Discord QR sign-in…',display_and_wait:{type:'nothing'}};const url=new URL('/_matrix/provision/v1/login/qr',b.url);url.protocol='ws:';url.searchParams.set('user_id',config.matrixAccount.userId);
  const socket=new WebSocket(url,{headers:{Authorization:'Bearer '+b.secret}});flows.set(id,{state,socket});socket.on('message',data=>{try{const d=JSON.parse(data);flows.get(id).state=d.code?{type:'display_and_wait',instructions:'Scan with Discord on your phone, then approve the login.',display_and_wait:{type:'qr',data:d.code}}:d.success?{type:'complete',instructions:'Discord connected'}:{type:'error',instructions:d.error||'Discord sign-in failed'}}catch{flows.get(id).state={type:'error',instructions:'Invalid response from Discord bridge'}}});socket.on('error',e=>{flows.get(id).state={type:'error',instructions:e.message}});return state;
 }
 const info=await call(config,id,'/v3/login/flows');const choices=info.flows||[];
 const selected=flow||choices.find(f=>/qr/i.test(f.id))?.id||choices[0]?.id;if(!selected)throw new Error('No sign-in flows are available');
 return accept(config,id,await call(config,id,'/v3/login/start/'+encodeURIComponent(selected),'POST',{}));
}
export async function submitBridgeLogin(config,id,values){
 const b=bridge(config,id),state=loginState(id);
 if(b.version===1){const r=await call(config,id,'/v1/login/token','POST',{token:values.token});const next={type:r.success?'complete':'error',instructions:r.success?'Discord connected':r.error};flows.set(id,{state:next});return next}
 if(!['cookies','user_input'].includes(state.type))throw new Error('This sign-in step does not accept text');
 const txn=state.txn_id?'?txn_id='+encodeURIComponent(state.txn_id):'';
 return accept(config,id,await call(config,id,`/v3/login/step/${encodeURIComponent(state.login_id)}/${encodeURIComponent(state.step_id)}/${state.type}${txn}`,'POST',values));
}
export async function guilds(config){return call(config,'discord','/v1/guilds')}
export async function enableGuild(config,id){if(!/^\d+$/.test(id))throw new Error('Invalid guild ID');return call(config,'discord','/v1/guilds/'+id,'POST',{auto_create_channels:true})}
