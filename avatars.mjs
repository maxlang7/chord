// Only profile photos belonging to identities already in Chord's directory.
// No arbitrary URLs or third-party avatar services are accepted.
export function avatarService({fetcher=fetch,now=Date.now}={}) {
 const cache=new Map();
 async function load(contact,account) {
  if(!contact||!account) return null;
  for(const identity of contact.identities||[]) {
   if(!/^@[^:]+:chord\.local$/.test(identity.key||''))continue;
   const headers={Authorization:'Bearer '+account.accessToken};
   const response=await fetcher(new URL('/_matrix/client/v3/profile/'+encodeURIComponent(identity.key)+'/avatar_url',account.homeserver),{headers,signal:AbortSignal.timeout(8000)});
   if(!response.ok)continue;
   const profile=await response.json();
   const match=/^mxc:\/\/([^/]+)\/([^/?#]+)$/.exec(profile.avatar_url||'');
   if(!match||match[1]!=='chord.local')continue;
   const path='/_matrix/client/v1/media/download/'+encodeURIComponent(match[1])+'/'+encodeURIComponent(match[2]);
   const image=await fetcher(new URL(path,account.homeserver),{headers,signal:AbortSignal.timeout(10000)});
   if(!image.ok)continue;
   const mimeType=(image.headers.get('content-type')||'').split(';')[0];
   if(!/^image\/(png|jpeg|gif|webp|bmp)$/.test(mimeType)||Number(image.headers.get('content-length')||0)>2*1024*1024)continue;
   const chunks=[];let size=0;
   for await(const chunk of image.body){size+=chunk.length;if(size>2*1024*1024)throw Object.assign(new Error('Profile photo exceeds 2 MB'),{status:413});chunks.push(chunk)}
   return {data:Buffer.concat(chunks).toString('base64url'),mimeType,size};
  }
  return null;
 }
 return async(contact,account)=>{
  const key=contact?.id;if(!key)return null;
  const previous=cache.get(key);if(previous&&previous.expires>now())return previous.promise;
  if(cache.size>=256)cache.delete(cache.keys().next().value);
  const promise=load(contact,account);cache.set(key,{promise,expires:now()+600000});
  try{return await promise}catch(e){cache.delete(key);throw e}
 };
}
