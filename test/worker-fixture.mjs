import {parentPort} from 'node:worker_threads';
parentPort.on('message',({id,action,args})=>{
 if(action==='block'){const end=Date.now()+200;while(Date.now()<end){};parentPort.postMessage({id,ok:true,data:'done'})}
 else if(action==='fail')parentPort.postMessage({id,ok:false,error:'Provider unavailable',status:503});
 else if(action==='close')parentPort.postMessage({id,ok:true,data:{closed:true}});
 else parentPort.postMessage({id,ok:true,data:args});
});
