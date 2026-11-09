// Local inbox rules never modify source-network read, mute, or subscription state.
export function applyAttention(items, rules, {server=null,query=''}={}) {
 if(!rules)return items;
 const shown=new Set(rules.visibleServerIDs||[]),allMessages=new Set(rules.allMessagesChannelIDs||[]),mentions=new Set(rules.mentionsOnlyServerIDs||[]);
 return items.filter(c=>c.source!=='discord'||server||query||(shown.has(c.serverID)&&(allMessages.has(c.channelID)||(mentions.has(c.serverID)&&c.unread&&Number(c.highlightCount||0)>0)))).map(c=>{
  if(c.source!=='discord')return c;
  const all=allMessages.has(c.channelID),ping=mentions.has(c.serverID)&&Number(c.highlightCount||0)>0;
  const unread=!!c.unread&&(all||ping);
  return {...c,sourceUnread:!!c.unread,unread,attention:{important:all||unread,reason:all?'All messages in a selected channel':mentions.has(c.serverID)?'Unread mentions only':'Quiet channel, available in All'}};
 });
}
