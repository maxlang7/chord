import {readFile,writeFile,mkdir,open} from 'node:fs/promises';
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {createHmac,randomBytes} from 'node:crypto';
import path from 'node:path';
import YAML from 'yaml';
const run=promisify(execFile),root=path.resolve(import.meta.dirname,'..'),dir=path.join(root,'private/runtime');
await mkdir(dir,{recursive:true,mode:0o700});process.umask(0o077);
const random=()=>randomBytes(32).toString('hex');
const secretsFile=path.join(dir,'secrets.json');
const secrets=await readFile(secretsFile,'utf8').then(JSON.parse).catch(()=>({registration:random(),password:random(),macaroon:random(),form:random(),bridges:{}}));
for(const id of ['whatsapp','slack','discord'])secrets.bridges[id]??={as:random(),hs:random(),provisioning:random()};
await writeFile(secretsFile,JSON.stringify(secrets,null,2),{mode:0o600});
const networks=[{id:'whatsapp',name:'WhatsApp',port:4811},{id:'slack',name:'Slack',port:4812},{id:'discord',name:'Discord',port:4813}];
for(const n of networks){
 const directory=path.join(dir,n.id);await mkdir(directory,{recursive:true,mode:0o700});const cfg=path.join(directory,'config.yaml');
 let exists=true;try{await readFile(cfg)}catch{exists=false}
 if(!exists){
  if(n.id==='discord')await writeFile(cfg,await readFile(process.env.CHORD_DISCORD_EXAMPLE),{mode:0o600});
  else await run('mautrix-'+n.id,['-e','-c',cfg],{cwd:directory,timeout:30000});
 }
 const c=YAML.parse(await readFile(cfg,'utf8'));c.homeserver={...c.homeserver,address:'http://127.0.0.1:4808',domain:'chord.local',software:'standard',websocket:false,status_endpoint:null,message_send_checkpoint_endpoint:null};
 c.appservice={...c.appservice,address:'http://127.0.0.1:'+n.port,hostname:'127.0.0.1',port:n.port,id:'chord-'+n.id,as_token:secrets.bridges[n.id].as,hs_token:secrets.bridges[n.id].hs};
 c.appservice.bot={...c.appservice.bot,username:n.id+'bot',displayname:'Chord '+n.name,avatar:'remove'};
 const database={type:'sqlite3-fk-wal',uri:'file:'+path.join(directory,'bridge.db')+'?_txlock=immediate',max_open_conns:5,max_idle_conns:1};
 c.bridge??={};c.bridge.permissions={'@max:chord.local':'admin'};c.bridge.bridge_matrix_leave=false;c.bridge.delete_guild_on_leave=false;
 if(c.logging)c.logging.min_level='info';
 if(n.id==='discord'){
  c.appservice.database=database;c.bridge.provisioning={...c.bridge.provisioning,prefix:'/_matrix/provision',shared_secret:secrets.bridges[n.id].provisioning,debug_endpoints:false};
  c.bridge.encryption={...c.bridge.encryption,allow:false,default:false,require:false};c.bridge.federate_rooms=false;c.bridge.startup_private_channel_create_limit=50;
  if(c.bridge.backfill){
   delete c.bridge.backfill.max_initial_messages;
   c.bridge.backfill.forward_limits={initial:{dm:100,channel:100,thread:50},missed:{dm:100,channel:100,thread:50}};
   c.bridge.backfill.max_guild_members=-1;
  }
 }else{
  c.database=database;c.provisioning={...c.provisioning,shared_secret:secrets.bridges[n.id].provisioning,allow_matrix_auth:true,debug_endpoints:false,enable_session_transfers:false};c.analytics={...c.analytics,token:null};c.matrix={...c.matrix,federate_rooms:false,message_status_events:true};c.encryption={...c.encryption,allow:true,default:true,require:true,appservice:true};
  // This private, non-federating homeserver has one permitted owner. Match
  // history-key sharing to the device policy already used for live messages.
  c.encryption.verification_levels={...c.encryption.verification_levels,share:'unverified'};
  c.backfill={...c.backfill,enabled:true,max_initial_messages:100};
  c.double_puppet={...c.double_puppet,secrets:{},servers:{'chord.local':'http://127.0.0.1:4808'},allow_discovery:false};
  c.network??={};if(n.id==='whatsapp'){c.network.os_name='Chord';c.network.browser_name='unknown'}
 }
 await writeFile(cfg,YAML.stringify(c),{mode:0o600});
 const registration={id:'chord-'+n.id,url:'http://127.0.0.1:'+n.port,as_token:secrets.bridges[n.id].as,hs_token:secrets.bridges[n.id].hs,sender_localpart:n.id+'bot',namespaces:{users:[{exclusive:true,regex:'^@'+n.id+'(?:bot|_.*):chord\\.local$'}],aliases:[{exclusive:true,regex:'^#'+n.id+'_.*:chord\\.local$'}],rooms:[]},rate_limited:false,'de.sorunome.msc2409.push_ephemeral':true,'org.matrix.msc3202':true};
 await writeFile(path.join(directory,'registration.yaml'),YAML.stringify(registration),{mode:0o600});
}
const hs={server_name:'chord.local',public_baseurl:'http://127.0.0.1:4808/',pid_file:path.join(dir,'synapse.pid'),listeners:[{port:4808,bind_addresses:['127.0.0.1'],type:'http',tls:false,x_forwarded:false,resources:[{names:['client'],compress:false}]}],database:{name:'sqlite3',args:{database:path.join(dir,'homeserver.db')}},media_store_path:path.join(dir,'media'),signing_key_path:path.join(dir,'chord.local.signing.key'),registration_shared_secret:secrets.registration,macaroon_secret_key:secrets.macaroon,form_secret:secrets.form,enable_registration:false,report_stats:false,trusted_key_servers:[],federation_domain_whitelist:[],app_service_config_files:networks.map(n=>path.join(dir,n.id,'registration.yaml')),max_upload_size:'25M',suppress_key_server_warning:true,rc_message:{per_second:100,burst_count:1000},rc_joins:{local:{per_second:50,burst_count:500}},experimental_features:{msc2409_to_device_messages_enabled:true,msc3202_device_masquerading:true}};
const hsConfig=path.join(dir,'homeserver.yaml');await writeFile(hsConfig,YAML.stringify(hs),{mode:0o600});
const children=[],logHandles=[];
async function start(name,command,args,cwd=dir){const log=await open(path.join(dir,name+'.log'),'a',0o600);logHandles.push(log);const child=spawn(command,args,{cwd,stdio:['ignore',log.fd,log.fd]});children.push(child);child.on('exit',code=>console.error(name+' exited '+code));return child}
for(const signal of ['SIGINT','SIGTERM'])process.on(signal,()=>{for(const child of children)child.kill('SIGTERM');setTimeout(()=>process.exit(),1000).unref()});
await start('synapse','synapse_homeserver',['-c',hsConfig]);
let ready=false;for(let i=0;i<90;i++){try{const r=await fetch('http://127.0.0.1:4808/_matrix/client/versions');if(r.ok){ready=true;break}}catch{}await new Promise(r=>setTimeout(r,1000))}
if(!ready)throw new Error('Local homeserver did not start. See private/runtime/synapse.log');
const registrationURL='http://127.0.0.1:4808/_synapse/admin/v1/register';
const {nonce}=await fetch(registrationURL).then(r=>r.json());
const mac=createHmac('sha1',secrets.registration).update([nonce,'max',secrets.password,'admin'].join('\0')).digest('hex');
const registrationResponse=await fetch(registrationURL,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({nonce,username:'max',password:secrets.password,admin:true,mac})});
const registered=await registrationResponse.json();if(!registrationResponse.ok&&registered.errcode!=='M_USER_IN_USE')throw new Error('Local account registration failed: '+registered.error);
const configFile=path.join(root,'private/config.json'),config=await readFile(configFile,'utf8').then(JSON.parse).catch(()=>({apiKey:random()}));
if(!config.matrixAccount){
 const response=await fetch('http://127.0.0.1:4808/_matrix/client/v3/login',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({type:'m.login.password',identifier:{type:'m.id.user',user:'max'},password:secrets.password,initial_device_display_name:'Chord'})});const account=await response.json();if(!response.ok)throw new Error('Local Matrix device login failed: '+account.error);
 config.matrixAccount={homeserver:'http://127.0.0.1:4808',userId:account.user_id,deviceId:account.device_id,accessToken:account.access_token};config.pickleKey=random();
}else if(config.matrixAccount.homeserver!=='http://127.0.0.1:4808')throw new Error('Another Matrix account exists. Refusing to overwrite its device.');
// Previous compatibility credentials are retained in an owner-only backup, never used by Chord.
if(config.access_token){await writeFile(path.join(dir,'unused-compatibility-config.json'),JSON.stringify(config),{mode:0o600});delete config.access_token;}
config.bridges=networks.map(n=>({...n,url:'http://127.0.0.1:'+n.port,version:n.id==='discord'?1:3,secret:secrets.bridges[n.id].provisioning}));
await writeFile(configFile,JSON.stringify(config,null,2),{mode:0o600});
for(const n of networks)await start(n.id,'mautrix-'+n.id,['-c',path.join(dir,n.id,'config.yaml')],path.join(dir,n.id));
console.log('Independent local homeserver and all three network bridges started. No Beeper services are configured.');
await new Promise(()=>{});
