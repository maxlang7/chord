import {readFile} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {convert} from 'html-to-text';

const configuration=await readFile(process.env.CHORD_CONFIG||path.join(import.meta.dirname,'private/config.json'),'utf8').then(JSON.parse).catch(()=>({}));
const definitions=(configuration.mailAccounts||[]).filter(a=>['home','cmu'].includes(a.id)&&a.email);
const activeTokens=new Map(),verified=new Set();
export const mailAccounts=()=>definitions.map(a=>({...a,kind:'email'}));
function account(id){const a=definitions.find(a=>a.id===id);if(!a)throw Object.assign(new Error('Unknown mail account'),{status:404});return a}
async function token(id,force=false){
 const a=account(id),cached=activeTokens.get(id);if(cached&&!force&&cached.expires>Date.now()+60000)return cached.token;
 const file=process.env.CHORD_MAIL_CREDENTIALS_DIR?path.join(process.env.CHORD_MAIL_CREDENTIALS_DIR,id+'.json'):a.credentialsFile||path.join(os.homedir(),'.config/google-workspace-'+id,'credentials',a.email+'.json');
 const credentials=JSON.parse(await readFile(file,'utf8'));
 const expiry=Date.parse(credentials.expiry?.endsWith('Z')?credentials.expiry:credentials.expiry+'Z');
 if(!force&&credentials.token&&expiry>Date.now()+60000){activeTokens.set(id,{token:credentials.token,expires:expiry});return credentials.token}
 if(!credentials.refresh_token)throw new Error('Google sign-in is required for '+a.email);
 const response=await fetch('https://oauth2.googleapis.com/token',{method:'POST',headers:{'Content-Type':'application/x-www-form-urlencoded'},body:new URLSearchParams({grant_type:'refresh_token',refresh_token:credentials.refresh_token,client_id:credentials.client_id,client_secret:credentials.client_secret}),signal:AbortSignal.timeout(20000)});
 const result=await response.json();if(!response.ok||!result.access_token)throw new Error('Google sign-in needs renewal for '+a.email);
 activeTokens.set(id,{token:result.access_token,expires:Date.now()+(result.expires_in||3600)*1000});return result.access_token;
}
async function request(id,endpoint,method='GET',body,retry=true){
 const response=await fetch((process.env.CHORD_GMAIL_TEST_URL||'https://gmail.googleapis.com/gmail/v1/users/me')+endpoint,{method,headers:{Authorization:'Bearer '+await token(id),'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(25000)});
 if(response.status===401&&retry){await token(id,true);return request(id,endpoint,method,body,false)}
 const result=await response.json();if(!response.ok)throw Object.assign(new Error(result.error?.message||'Gmail request failed'),{status:response.status});return result;
}
async function verify(id){if(verified.has(id))return;const profile=await request(id,'/profile');if(profile.emailAddress?.toLowerCase()!==account(id).email.toLowerCase())throw new Error('Google account mismatch. Refusing to read another inbox.');verified.add(id)}
function header(message,name){return message.payload?.headers?.find(h=>h.name.toLowerCase()===name.toLowerCase())?.value||''}
function decode(s){return Buffer.from(s||'','base64url').toString('utf8')}
function content(payload){let plain=[],html=[];function visit(p){if(p.mimeType==='text/plain'&&p.body?.data)plain.push(decode(p.body.data));if(p.mimeType==='text/html'&&p.body?.data)html.push(decode(p.body.data));for(const child of p.parts||[])visit(child)}visit(payload);return plain.join('\n')||convert(html.join('\n'),{wordwrap:false,selectors:[{selector:'img',format:'skip'}]})}
function attachments(payload){const result=[];function visit(p){if(p.filename&&p.body?.attachmentId)result.push({id:p.body.attachmentId,fileName:p.filename,mimeType:p.mimeType,fileSize:p.body.size});for(const child of p.parts||[])visit(child)}visit(payload);return result}
function preview(m){return {id:m.id,threadId:m.threadId,subject:header(m,'subject')||'(No subject)',from:header(m,'from'),to:header(m,'to'),date:new Date(Number(m.internalDate)||Date.now()).toISOString(),snippet:m.snippet||'',unread:m.labelIds?.includes('UNREAD')||false,labels:m.labelIds||[],automated:!!header(m,'list-id')||!!header(m,'list-unsubscribe')||/bulk|list/i.test(header(m,'precedence'))||(!['','no'].includes(header(m,'auto-submitted').toLowerCase()))}}
export async function listMail(id,{query='in:inbox',cursor,limit=30}={}){
 await verify(id);const q=new URLSearchParams({q:query,maxResults:String(Math.min(100,limit))});if(cursor)q.set('pageToken',cursor);
 const list=await request(id,'/threads?'+q),threads=[];
 for(let i=0;i<(list.threads||[]).length;i+=6){const batch=await Promise.all((list.threads||[]).slice(i,i+6).map(t=>request(id,'/threads/'+t.id+'?format=metadata&metadataHeaders=From&metadataHeaders=To&metadataHeaders=Subject&metadataHeaders=Date&metadataHeaders=List-ID&metadataHeaders=List-Unsubscribe&metadataHeaders=Auto-Submitted&metadataHeaders=Precedence')));for(const thread of batch){const latest=thread.messages?.at(-1);if(latest)threads.push({...preview(latest),id:thread.id,unread:thread.messages.some(m=>m.labelIds?.includes('UNREAD')),count:thread.messages.length,account:id})}}
 return {items:threads,hasMore:!!list.nextPageToken,oldestCursor:list.nextPageToken||null};
}
export async function readMail(id,threadId){await verify(id);const thread=await request(id,'/threads/'+encodeURIComponent(threadId)+'?format=full');return {items:(thread.messages||[]).map(m=>({...preview(m),text:content(m.payload),messageIdHeader:header(m,'message-id'),references:header(m,'references'),attachments:attachments(m.payload),isSender:header(m,'from').toLowerCase().includes(account(id).email.toLowerCase())})),hasMore:false,oldestCursor:null}}
export async function modifyMail(id,threadId,{read,archived}){await verify(id);const add=[],remove=[];if(read===true)remove.push('UNREAD');if(read===false)add.push('UNREAD');if(archived===true)remove.push('INBOX');if(archived===false)add.push('INBOX');return request(id,'/threads/'+encodeURIComponent(threadId)+'/modify','POST',{addLabelIds:add,removeLabelIds:remove})}
export async function mailAttachment(id,messageId,attachmentId){await verify(id);return request(id,'/messages/'+encodeURIComponent(messageId)+'/attachments/'+encodeURIComponent(attachmentId))}
export async function sendMail(id,{to,subject,text,threadId,replyToMessageId,references,confirmSend}){
 await verify(id);if(confirmSend!==true)throw Object.assign(new Error('Sending email requires an explicit confirmation.'),{status:400});
 if(!text?.trim())throw new Error('Email text is empty');for(const value of [to,subject,replyToMessageId,references])if(value&&/[\r\n]/.test(value))throw new Error('Email headers cannot contain line breaks');if(!to||!to.includes('@'))throw new Error('A recipient is required');
 const headers=['From: '+account(id).email,'To: '+to,'Subject: =?UTF-8?B?'+Buffer.from(subject||'').toString('base64')+'?=','MIME-Version: 1.0','Content-Type: text/plain; charset=UTF-8','Content-Transfer-Encoding: base64'];if(replyToMessageId)headers.push('In-Reply-To: '+replyToMessageId);if(references)headers.push('References: '+references);
 const raw=Buffer.from(headers.join('\r\n')+'\r\n\r\n'+Buffer.from(text).toString('base64').replace(/.{1,76}/g,'$&\r\n')).toString('base64url');
 return request(id,'/messages/send','POST',{raw,...(threadId?{threadId}:{})});
}
