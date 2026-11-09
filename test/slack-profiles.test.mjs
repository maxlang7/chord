import test from 'node:test';
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {mkdtemp,mkdir,rm} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {cacheSlackProfiles} from '../slack-profiles.mjs';
import {ContactBook} from '../contacts.mjs';
test('workspace avatars paginate, cache every profile, and never forward credentials to photo hosts',async t=>{
 const dir=await mkdtemp(path.join(os.tmpdir(),'chord-profiles-'));t.after(()=>rm(dir,{recursive:true,force:true}));await mkdir(path.join(dir,'runtime/slack'),{recursive:true});
 const db=new DatabaseSync(path.join(dir,'runtime/slack/bridge.db'));db.exec('CREATE TABLE user_login(id TEXT,metadata TEXT)');db.prepare('INSERT INTO user_login VALUES (?,?)').run('TTEST-UOWNER',JSON.stringify({token:'test-secret',cookie_token:'test-cookie'}));db.close();
 const book=new ContactBook(dir);book.save=async()=>{};const saved=new Map();let pages=0;
 const fetcher=async(url,options)=>{
  if(url.startsWith('https://slack.com/api/')){
   assert.equal(options.headers.Authorization,'Bearer test-secret');
   if(url.endsWith('auth.test'))return Response.json({ok:true,team_id:'TTEST'});
   assert.ok(url.endsWith('users.list'));pages++;
   return Response.json(pages===1?{ok:true,members:[{id:'U1',profile:{image_512:'https://avatars.slack-edge.com/one'}}],response_metadata:{next_cursor:'next'}}:{ok:true,members:[{id:'U2',profile:{image_512:'https://secure.gravatar.com/two'}},{id:'U3',profile:{}}]});
  }
  assert.equal(options.headers,undefined);return new Response(new Uint8Array([1,2,3]),{headers:{'content-type':'image/png'}});
 };
 const r=await cacheSlackProfiles('TTEST',{dataDir:dir,directory:{book},cache:{put:(k,v)=>saved.set(k,v)},fetcher});
 assert.equal(pages,2);assert.equal(r.profiles,3);assert.equal(r.cached,2);assert.equal(r.noPhoto,1);assert.deepEqual(r.failures,[]);assert.equal(saved.size,2);
});
