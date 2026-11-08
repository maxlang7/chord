// Only expose connection state here. Never return bridge tokens or provisioning responses.
export async function connectionStates({config, bridgeInfo, mailAccounts, checkMail, blue}) {
 const checkedAt=new Date().toISOString();
 const items=await Promise.all([
  ...(config.bridges||[]).map(async b=>{
   try {
    const info=await bridgeInfo(config,b.id);
    const logins=Object.values(info.logins||info.user_logins||{}).flat();
    const discord=info.Discord||info;
    const connected=b.version===1?discord.logged_in===true&&discord.connected!==false:logins.some(l=>(l.state_event||l.state?.state_event)==='CONNECTED');
    return {id:b.id,name:b.name,connected,state:connected?'connected':'sign-in',detail:connected?'Bridge session available':'Sign in to this network in Chord',canConnect:true};
   }catch(e){return {id:b.id,name:b.name,connected:false,state:'offline',detail:e.message,canConnect:true}}
  }),
  ...mailAccounts().map(async a=>{
   try {await checkMail(a.id);return {id:'mail:'+a.id,name:a.id==='home'?'Personal email':'CMU email',email:a.email,connected:true,state:'connected',detail:'Gmail account verified',canConnect:false}}
   catch(e){return {id:'mail:'+a.id,name:a.id==='home'?'Personal email':'CMU email',email:a.email,connected:false,state:'offline',detail:e.message,canConnect:false}}
  }),
  (async()=>{try{const s=await blue('GetStatus');return {id:'iphone',name:'iPhone',connected:!!s.map,state:s.map?'connected':'offline',detail:s.map?'Bluetooth messages available':s.connectivity_detail||'Reconnect your iPhone over Bluetooth',cached:true,canConnect:false}}catch(e){return {id:'iphone',name:'iPhone',connected:false,state:'offline',detail:'BlueFerry is unavailable',canConnect:false}}})(),
 ]);
 return {checkedAt,items};
}
