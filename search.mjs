export async function searchMessages(query,{matrix,blue,mailAccounts,listMail,directory}) {
 query=query.trim();
 if(!query)return {items:[],errors:[],scope:'Search text is empty'};
 const items=[],errors=[];
 const fail=(source,e)=>errors.push({source,error:e?.message||String(e)});
 await Promise.all([
  (async()=>{try {
   const page=await matrix('/v1/messages/search?query='+encodeURIComponent(query)+'&limit=20');
   for(const m of page.items||[]){
    const id='matrix:'+m.chatID;
    const c=directory.conversations.get(id);
    if(c)items.push({conversation:c,message:m});
   }
  }catch(e){fail('matrix',e)}})(),
  (async()=>{try {
   const threads=await blue('ListThreads','u',[200]);
   for(const t of threads){const id='blueferry:'+t.key,c=directory.conversations.get(id);if(!c)continue;
    for(const m of t.messages||[])if(m.body?.toLowerCase().includes(query.toLowerCase()))items.push({conversation:c,message:{id:m.handle||'',text:m.body,timestamp:m.timestamp,senderName:m.sender||t.name}});
   }
  }catch(e){fail('iphone',e)}})(),
  ...mailAccounts().map(async a=>{try {
   const page=await listMail(a.id,{query,limit:15});
   for(const m of page.items){const c=directory.enrichConversation({...m,id:`mail:${a.id}:${m.id}`,remoteId:m.id,title:m.subject,preview:m.snippet,source:'mail:'+a.id,kind:'email',timestamp:m.date,canSend:true,canReply:true,canReact:false,participants:[]});items.push({conversation:c,message:{id:'',text:m.snippet,senderName:m.from,timestamp:m.date}})}
  }catch(e){fail('mail:'+a.id,e)}}),
 ]);
 items.sort((a,b)=>Date.parse(b.message.timestamp)-Date.parse(a.message.timestamp));
 return {items:items.slice(0,60),errors,scope:'Chat search covers messages synced or opened in Chord. Email searches Gmail. iPhone searches cached Bluetooth messages.'};
}
