import {Worker} from 'node:worker_threads';
// Crypto/WASM catch-up can occupy its entire JS thread. Keep it off the HTTP
// thread so Gmail, phone messages, health checks, and errors stay responsive.
export class MatrixTransport {
 constructor(context,{workerURL=new URL('./matrix-worker.mjs',import.meta.url),timeout=120000}={}){
  this.worker=new Worker(workerURL,{workerData:context});this.pending=new Map();this.seq=0;this.timeout=timeout;
  this.worker.on('message',m=>{const p=this.pending.get(m.id);if(!p)return;clearTimeout(p.timer);this.pending.delete(m.id);m.ok?p.resolve(m.data):p.reject(Object.assign(new Error(m.error),{status:m.status}))});
  this.worker.on('error',e=>{this.closed=true;this.fail(e)});this.worker.on('exit',()=>{this.closed=true;this.fail(new Error('Matrix engine stopped. Reopen Chord to reconnect.'))});
 }
 fail(error){for(const p of this.pending.values()){clearTimeout(p.timer);p.reject(error)}this.pending.clear()}
 call(action,...args){
  if(this.closed)return Promise.reject(new Error('Matrix engine is offline'));
  if(this.pending.size>=64)return Promise.reject(Object.assign(new Error('Matrix is catching up. Try again shortly.'),{status:429}));
  const id=++this.seq;
  return new Promise((resolve,reject)=>{
   const timer=setTimeout(()=>{this.pending.delete(id);reject(Object.assign(new Error(action==='request'&&args[1]!=='GET'?'Matrix request timed out. Check the conversation before retrying a send.':'Matrix is still syncing history. Gmail and iPhone remain available.'),{status:504}))},this.timeout);
   this.pending.set(id,{resolve,reject,timer});this.worker.postMessage({id,action,args});
  });
 }
 async close(){await Promise.race([this.call('close'),new Promise(r=>setTimeout(r,1500))]).catch(()=>{});await this.worker.terminate()}
}
let transport;
export function matrixAction(context,action,...args){if(!transport||transport.closed)transport=new MatrixTransport(context);return transport.call(action,...args)}
export async function closeMatrixWorker(){if(transport)await transport.close();transport=null}
