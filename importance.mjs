const DAY=86400000;
export function importance(conversation,{preference={},favorite=false,now=Date.now()}={}){
 const c=conversation,signals=c.signals||{},activity=c.activity||{};
 const stamp=Math.max(Number(activity.lastMessageAt)||0,Date.parse(c.timestamp)||0),age=(now-stamp)/DAY;
 const reasons=[];let score=0;
 const add=(points,reason)=>{score+=points;reasons.push(reason)};
 if(preference.muted)return {important:false,score:-500,reasons:['Muted'],rule:'muted'};
 if(c.attention)return {important:c.attention.important,score:c.attention.important?100:0,reasons:[c.attention.reason],rule:'attention'};
 if(preference.priority==='important')return {important:true,score:1000,reasons:['Pinned by you'],rule:'important'};
 if(preference.priority==='quiet')return {important:false,score:-1000,reasons:['Quieted by you'],rule:'quiet'};
 if(c.isMuted)return {important:false,score:-500,reasons:['Muted conversation'],rule:'auto'};
 if(favorite)add(100,'Favorite contact');
 if(signals.mention&&age<=30)add(90,'Mentions you');
 if(signals.replyToYou&&age<=30)add(85,'Reply to your message');
 if(c.kind==='email'){
  const labels=c.labels||[];
  if(labels.includes('STARRED'))add(95,'Starred email');
  if(labels.includes('CATEGORY_PROMOTIONS')||labels.includes('CATEGORY_SOCIAL')||labels.includes('SPAM'))return {important:score>=90,score,reasons:reasons.length?reasons:['Promotional or social email'],rule:'auto'};
  if(labels.includes('IMPORTANT')&&age<=30)add(65,'Marked important in email');
  if(c.unread&&!signals.automated&&age<=30)add(65,'Unread personal email');
  if(!signals.automated&&c.knownContact&&age<=14)add(55,'Recent email from a contact');
 }else if(!c.isBot){
  if(c.direct&&((c.unread&&age<=60)||age<=14))add(c.unread?75:55,c.unread?'Unread direct message':'Recent direct conversation');
  if(Number(activity.lastSentAt)>now-30*DAY&&age<=14)add(55,'You participate in this conversation');
 }
 if(c.unread&&score>0)add(5,'Unread');
 if(!reasons.length)reasons.push(c.isBot?'Automated account':age>60?'Older conversation':'Channel without a recent mention or participation');
 return {important:score>=50,score,reasons,rule:'auto'};
}

export function applyLocalRules(items,book,now=Date.now()){
 return items.map(c=>withLocalRules(c,book,now));
}
function withLocalRules(c,book,now){
 const preference=book.preference(c.id,now);
 if(!preference.muted)return {...c,localPriority:preference.priority,muteUntil:null};
 return {...c,localPriority:preference.priority,muteUntil:preference.muteUntil,isMuted:true,unread:false,sourceUnread:c.sourceUnread??!!c.unread};
}
export function rankInbox(items,book,{view='all',now=Date.now()}={}){
 const ranked=items.map(c=>{
  const next=withLocalRules(c,book,now);
  const favorite=(next.contactIDs||[]).some(id=>book.data.people[id]?.favorite)&&(next.direct||next.kind==='email');
  return {...next,importance:importance(next,{preference:book.preference(c.id,now),favorite,now})};
 });
 const focused=ranked.filter(c=>c.importance.important);
 const visible=view==='focus'?focused:ranked;
 const timestamp=c=>{const value=Date.parse(c.timestamp);return Number.isFinite(value)?value:0};
 visible.sort((a,b)=>timestamp(b)-timestamp(a));
 return {items:visible,summary:{total:ranked.length,important:focused.length,hidden:view==='focus'?ranked.length-focused.length:0},view};
}
