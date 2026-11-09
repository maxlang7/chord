import {Worker} from 'node:worker_threads';
import path from 'node:path';
import {ArchiveStore,accountNamespace} from './archive-store.mjs';
export class ArchiveService {
 constructor(dataDir,config){
  this.store=new ArchiveStore(path.join(dataDir,'archive.sqlite'),{namespace:accountNamespace(config)});this.state='starting';this.dataDir=dataDir;
 }
 start(){
  this.worker=new Worker(new URL('./archive-worker.mjs',import.meta.url),{workerData:{dataDir:this.dataDir,namespace:this.store.namespace}});
  this.worker.on('message',m=>{this.state=m.state||m.type;if(m.error)this.error=m.error});
  this.worker.on('error',e=>{this.error=e.message;this.state='error'});
 }
 status(){return {...this.store.stats(),worker:this.state,lastError:this.error||null}}
 maintain(){this.worker.postMessage('maintain')}
 async close(){await this.worker?.terminate();this.store.close()}
}
