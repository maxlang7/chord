import {readFile,writeFile,rename} from 'node:fs/promises';
import path from 'node:path';
import {brotliCompress,brotliDecompress,constants} from 'node:zlib';
import {promisify} from 'node:util';
const compress=promisify(brotliCompress),decompress=promisify(brotliDecompress);
export async function readHistory(dataDir){
 try{return JSON.parse((await decompress(await readFile(path.join(dataDir,'matrix-timeline.json.br')))).toString())}
 catch(e){if(e.code!=='ENOENT')throw e;return JSON.parse(await readFile(path.join(dataDir,'matrix-timeline.json'),'utf8'))}
}
export async function writeHistory(dataDir,value){
 const packed=await compress(Buffer.from(JSON.stringify(value)),{params:{[constants.BROTLI_PARAM_QUALITY]:3}});
 const file=path.join(dataDir,'matrix-timeline.json.br');await writeFile(file+'.pending',packed,{mode:0o600});await rename(file+'.pending',file);
}
export async function* allSavedHistory(dataDir){
 // Keep importing the preserved original too, even after startup snapshots
 // contain only the last 30 days. Imports are idempotent and cannot replace live versions.
 for(const name of ['matrix-timeline.json','matrix-timeline.json.br']){
  let bytes;try{bytes=await readFile(path.join(dataDir,name))}catch(e){if(e.code==='ENOENT')continue;throw e}
  yield JSON.parse((name.endsWith('.br')?await decompress(bytes):bytes).toString());
 }
}
