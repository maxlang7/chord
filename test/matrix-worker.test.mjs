import test from 'node:test';
import assert from 'node:assert/strict';
import {MatrixTransport} from '../matrix-transport.mjs';
test('CPU-heavy Matrix work leaves the main event loop responsive and provider errors propagate',async()=>{
 const t=new MatrixTransport({}, {workerURL:new URL('./worker-fixture.mjs',import.meta.url)});
 try {
  let finished=false;const work=t.call('block').then(d=>{finished=true;return d});let ticked=false;
  await new Promise(r=>setTimeout(()=>{ticked=true;r()},20));assert.equal(ticked,true);assert.equal(finished,false);
  assert.equal(await work,'done');
  await assert.rejects(t.call('fail'),{message:'Provider unavailable',status:503});
 }finally{await t.close()}
});
test('a stalled worker has a bounded request timeout',async()=>{
 const t=new MatrixTransport({}, {workerURL:new URL('./worker-fixture.mjs',import.meta.url),timeout:20});
 try{await assert.rejects(t.call('block'),e=>e.status===504)}finally{await t.close()}
});
