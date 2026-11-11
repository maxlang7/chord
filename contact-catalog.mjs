import {DatabaseSync} from 'node:sqlite';
import path from 'node:path';

// Read bridge-owned databases through read-only handles. Select only names,
// identifiers, and activity counts; authentication columns are never read.
export function readContactCatalog(dataDir, owner='@max:chord.local') {
 const identities=[],servers=[],rooms=new Map(),selfIds=new Set([owner]);
 const errors=[];
 for(const network of ['slack','discord','whatsapp']){
  let db;
  try{
   db=new DatabaseSync(path.join(dataDir,'runtime',network,'bridge.db'),{readOnly:true});
   const query=(sql,...args)=>db.prepare(sql).all(...args);
   let mine=[];
   if(network==='discord'){
    mine=query('SELECT dcid FROM user WHERE mxid=?',owner).map(r=>r.dcid).filter(Boolean);
    for(const p of query('SELECT id,name,global_name,username,is_bot FROM puppet'))identities.push({key:`@discord_${p.id}:chord.local`,source:network,name:p.global_name||p.name||p.username,isBot:!!p.is_bot,handle:p.username});
    for(const g of query("SELECT dcid,plain_name,name,mxid FROM guild"))servers.push({id:g.dcid,name:g.plain_name||g.name,imported:!!g.mxid});
    for(const r of query("SELECT p.dcid,p.receiver,p.mxid,p.plain_name,p.type,p.other_user_id,p.dc_guild_id,g.plain_name AS guild_name FROM portal p LEFT JOIN guild g ON g.dcid=p.dc_guild_id WHERE p.mxid IS NOT NULL AND p.mxid!='' AND p.type!=4"))rooms.set(r.mxid,{network,title:r.plain_name,direct:r.type===1,groupDM:r.type===3,serverID:r.dc_guild_id||null,serverName:r.guild_name||null,otherUser:r.other_user_id?`@discord_${r.other_user_id}:chord.local`:null,portalId:r.dcid,receiver:r.receiver});
   }else{
    mine=query('SELECT id FROM user_login WHERE user_mxid=?',owner).map(r=>r.id.toLowerCase());
    const workspaces=network==='slack'?new Map(query('SELECT id,remote_name FROM user_login').map(r=>[r.id.split('-')[0].toLowerCase(),r.remote_name?.split(' - ')[0]])):new Map();
    for(const p of query('SELECT id,name,is_bot,identifiers FROM ghost')){
     let aliases=[];try{aliases=JSON.parse(p.identifiers)||[]}catch{}
     identities.push({key:`@${network}_${p.id}:chord.local`,source:network,name:p.name,rank:network==='slack'?80:50,isBot:!!p.is_bot,label:workspaces.get(p.id.split('-')[0].toLowerCase()),aliases:aliases.filter(x=>typeof x==='string')});
    }
    for(const r of query("SELECT id,receiver,mxid,name,room_type,other_user_id FROM portal WHERE mxid IS NOT NULL AND mxid!='' AND room_type!='space'"))rooms.set(r.mxid,{network,title:r.name,direct:r.room_type==='dm',groupDM:r.room_type==='group_dm',otherUser:r.other_user_id?`@${network}_${r.other_user_id}:chord.local`:null,portalId:r.id,receiver:r.receiver});
    if(network==='whatsapp'){
     for(const m of query('SELECT lid,pn FROM whatsmeow_lid_map')){
      const pn=m.pn.replace(/@.+$/,'');
      const phone='+'+pn;
      identities.push({key:`@whatsapp_lid-${m.lid.replace(/@.+$/,'')}:chord.local`,source:network,aliases:[`tel:${phone}`]});
      identities.push({key:`@whatsapp_${pn}:chord.local`,source:network,aliases:[`tel:${phone}`]});
      if(mine.includes(pn))mine.push('lid-'+m.lid.replace(/@.+$/,''));
     }
     for(const p of query('SELECT their_jid,full_name,first_name,push_name,business_name FROM whatsmeow_contacts')){
      const [number,server]=p.their_jid.split('@');
      const key=`@whatsapp_${server==='lid'?'lid-':''}${number}:chord.local`;
      identities.push({key,source:network,name:p.full_name||p.first_name||p.push_name||p.business_name,...(server==='s.whatsapp.net'?{aliases:['tel:+'+number]}:{})});
     }
    }
   }
   for(const id of mine)selfIds.add(`@${network}_${id}:chord.local`);
   // Matrix lazy membership often contains only one human. Use known authors
   // for a stable, bounded set of representative group photos.
   const authors=network==='discord'
    ? query('SELECT dc_chan_id AS portal_id,dc_chan_receiver AS receiver,dc_sender AS sender,COUNT(*) AS total FROM message GROUP BY dc_chan_id,dc_chan_receiver,dc_sender ORDER BY total DESC,sender')
    : query('SELECT room_id AS portal_id,room_receiver AS receiver,sender_id AS sender,COUNT(*) AS total FROM message GROUP BY room_id,room_receiver,sender_id ORDER BY total DESC,sender');
   const members=new Map();
   for(const author of authors){
    const sender=String(author.sender||'');if(!sender||mine.includes(sender.toLowerCase()))continue;
    const key=author.portal_id+'|'+author.receiver,list=members.get(key)||[];
    if(list.length<12)list.push(`@${network}_${sender}:chord.local`);
    members.set(key,list);
   }
   for(const room of rooms.values())if(room.network===network&&!room.direct)room.avatarMembers=members.get(room.portalId+'|'+room.receiver)||[];
   const placeholders=mine.length?mine.map(()=>'?').join(','):'NULL';
   const stats=network==='discord'
    ? query(`SELECT dc_chan_id AS portal_id,dc_chan_receiver AS receiver,COUNT(*) AS total,MAX(timestamp * 1.0) AS last_at,SUM(CASE WHEN dc_sender IN (${placeholders}) THEN 1 ELSE 0 END) AS sent,MAX(CASE WHEN dc_sender IN (${placeholders}) THEN timestamp * 1.0 ELSE 0 END) AS last_sent FROM message GROUP BY dc_chan_id,dc_chan_receiver`,...mine,...mine)
    : query(`SELECT room_id AS portal_id,room_receiver AS receiver,COUNT(*) AS total,MAX(timestamp * 1.0) AS last_at,SUM(CASE WHEN LOWER(sender_id) IN (${placeholders}) OR sender_mxid=? THEN 1 ELSE 0 END) AS sent,MAX(CASE WHEN LOWER(sender_id) IN (${placeholders}) OR sender_mxid=? THEN timestamp * 1.0 ELSE 0 END) AS last_sent FROM message GROUP BY room_id,room_receiver`,...mine,owner,...mine,owner);
   const byPortal=new Map(stats.map(s=>[s.portal_id+'|'+s.receiver,s]));
   for(const r of rooms.values())if(r.network===network){
    const s=byPortal.get(r.portalId+'|'+r.receiver);if(!s)continue;
    // Bridge v2 stores nanoseconds; the older Discord bridge stores milliseconds.
    const ms=n=>{n=Number(n||0);return n>1e16?n/1e6:n>1e13?n/1000:n>0&&n<1e11?n*1000:n};
    r.activity={totalMessages:Number(s.total),sentCount:Number(s.sent||0),lastMessageAt:ms(s.last_at),lastSentAt:ms(s.last_sent)};
   }
  }catch(e){errors.push({source:network,error:e.code||e.name})}finally{db?.close()}
 }
 return {identities,servers,rooms,selfIds,errors};
}
