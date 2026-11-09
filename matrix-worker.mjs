import {parentPort,workerData} from 'node:worker_threads';
import {getMatrix,matrixRequest,closeMatrix} from './matrix-provider.mjs';
parentPort.on('message',async ({id,action,args})=>{
 try {
  let data;
  if(action==='request')data=await matrixRequest(workerData,...args);
  else if(action==='close'){await closeMatrix();data={closed:true}}
  else {
   const p=await getMatrix(workerData);
   if(action==='refresh'){await p.refreshRooms();data={success:true}}
   else if(action==='crypto')data={crypto:await p.client.crypto.status(),lastError:p.error};
   else if(action==='attachment'){
    const [roomId,eventId,index,expectedID]=args;
    const message=await p.client.messages.get({roomId,eventId});
    const attachment=p.normalizeMessage(message.message).attachments[index];
    if(!attachment)throw Object.assign(new Error('Attachment not found'),{status:404});
    if(expectedID&&attachment.id!==expectedID)throw Object.assign(new Error('The attachment changed. Refresh the conversation before downloading it.'),{status:409});
    if(attachment.fileSize>20*1024*1024)throw Object.assign(new Error('Attachment exceeds 20 MB'),{status:413});
    const result=await p.download(attachment.id),bytes=Buffer.from(result.bytes);
    if(bytes.length>20*1024*1024)throw Object.assign(new Error('Attachment exceeds 20 MB'),{status:413});
    data={data:bytes.toString('base64url'),fileName:attachment.fileName,mimeType:attachment.mimeType,size:bytes.length};
   }else if(action==='diagnostics'){
    const [roomId]=args;
    const raw=await p.raw('GET',`/_matrix/client/v3/rooms/${encodeURIComponent(roomId)}/messages`,undefined,{dir:'b',limit:'50'});
    const page=await p.client.messages.list({roomId,limit:50,direction:'backward'});
    data={rawCount:raw.chunk?.length,rawTypes:(raw.chunk||[]).reduce((c,e)=>(c[e.type]=(c[e.type]||0)+1,c),{}),messageCount:page.messages?.length,crypto:await p.client.crypto.status(),lastError:p.error};
   }else throw new Error('Unknown Matrix worker action');
  }
  parentPort.postMessage({id,ok:true,data});
 }catch(e){parentPort.postMessage({id,ok:false,error:e?.message||String(e),status:e?.status||503})}
});
