// Connection diagnostics belong in account status, not the human-message inbox.
// Retained history is preserved; never classify a real phone thread by its text.
export function isStatusConversation(c,owner){
 if(c.kind==='phone'||c.source==='iphone')return false;
 if(/blueferry/i.test(c.source||'')&&c.kind==='chat')return true;
 if(c.source!=='matrix')return false;
 const participants=(c.participants||[]).map(p=>typeof p==='string'?p:p.id);
 const isBot=id=>/^@(slack|discord|whatsapp|blueferry)bot:chord\.local$/.test(id||'');
 if(participants.some(isBot)&&participants.every(id=>id===owner||isBot(id)))return true;
 return !!owner&&participants.length>0&&participants.every(id=>id===owner)&&/^(BlueFerry|WhatsApp|Slack|Discord) (connection|bridge status|status updates)$/i.test(c.title||'');
}
