import {DatabaseSync} from 'node:sqlite';
import path from 'node:path';
const MAX_BYTES=2*1024*1024;
export async function cacheSlackProfiles(workspace,{dataDir,directory,cache,fetcher=fetch,photos=true}){
 if(!/^[A-Z0-9]+$/.test(workspace||''))throw new Error('Invalid Slack workspace');
 const db=new DatabaseSync(path.join(dataDir,'runtime/slack/bridge.db'),{readOnly:true});let login;
 try{login=db.prepare('SELECT metadata FROM user_login WHERE UPPER(id) LIKE ?').get(workspace+'-%')}finally{db.close()}
 if(!login)throw new Error('Slack workspace is not connected');
 const credentials=JSON.parse(login.metadata);
 async function slack(method,fields={}){
  const r=await fetcher('https://slack.com/api/'+method,{method:'POST',headers:{Authorization:'Bearer '+credentials.token,Cookie:'d='+encodeURIComponent(credentials.cookie_token),'Content-Type':'application/x-www-form-urlencoded'},body:new URLSearchParams(fields),signal:AbortSignal.timeout(30000)});
  if(!r.ok)throw new Error('Slack profile request failed: HTTP '+r.status);
  const d=await r.json();if(!d.ok)throw new Error('Slack profile request failed: '+d.error);return d;
 }
 const auth=await slack('auth.test');if(auth.team_id!==workspace)throw new Error('Slack workspace identity mismatch');
 const members=new Map();let cursor='';
 do{const page=await slack('users.list',{team_id:workspace,limit:'200',...(cursor?{cursor}:{})});for(const user of page.members||[])members.set(user.id,user);cursor=page.response_metadata?.next_cursor||''}while(cursor);
 const jobs=[...members.values()],result={workspace,profiles:members.size,namesSynced:0,cached:0,noPhoto:0,failures:[]};
 async function worker(){while(jobs.length){const u=jobs.shift(),profile=u.profile||{};const url=profile.image_512||profile.image_192||profile.image_original;
  const preferredName=profile.display_name||profile.real_name||u.real_name||u.name;
  const person=directory.book.observe({key:`@slack_${workspace.toLowerCase()}-${u.id.toLowerCase()}:chord.local`,source:'slack',name:preferredName,rank:/\s/.test(preferredName||'')?85:70,isBot:!!u.is_bot,label:'Slack '+workspace});
  result.namesSynced++;if(!photos)continue;
  if(!url){result.noPhoto++;continue}
  try{
   const parsed=new URL(url);if(parsed.protocol!=='https:'||!(parsed.hostname==='secure.gravatar.com'||['slack-edge.com','slack.com','slack-imgs.com'].some(h=>parsed.hostname===h||parsed.hostname.endsWith('.'+h))))throw new Error('Unsupported profile photo host');
   const r=await fetcher(url,{signal:AbortSignal.timeout(20000)});if(!r.ok)throw new Error('Photo HTTP '+r.status);
   const mimeType=(r.headers.get('content-type')||'').split(';')[0];if(!/^image\/(png|jpeg|gif|webp|bmp)$/.test(mimeType))throw new Error('Unsupported photo type');
   if(Number(r.headers.get('content-length')||0)>MAX_BYTES)throw new Error('Profile photo exceeds 2 MB');
   let size=0;const chunks=[];for await(const part of r.body){size+=part.length;if(size>MAX_BYTES)throw new Error('Profile photo exceeds 2 MB');chunks.push(part)}
   cache.put('media:avatar:'+person.id,{data:Buffer.concat(chunks).toString('base64url'),mimeType,size},{kind:'media'});result.cached++;
  }catch(e){result.failures.push({identity:u.id,error:e.message})}
 }}
 await Promise.all([worker(),worker(),worker()]);await directory.book.save();return result;
}
