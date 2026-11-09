// Profile pictures only. Does not read message attachments or mark conversations read.
import {readFile,writeFile} from 'node:fs/promises';
const workspace=process.argv[2];if(!/^[A-Z0-9]+$/.test(workspace||''))throw new Error('Provide a Slack workspace ID');
const config=JSON.parse(await readFile(new URL('../private/config.json',import.meta.url)));
const r=await fetch('http://127.0.0.1:4789/api/slack/cache-profiles',{method:'POST',headers:{Authorization:'Bearer '+config.apiKey,'Content-Type':'application/json'},body:JSON.stringify({workspace}),signal:AbortSignal.timeout(600000)});
const report=await r.json();await writeFile(new URL('../private/workspace-avatar-cache.json',import.meta.url),JSON.stringify(report,null,2),{mode:0o600});
console.log(JSON.stringify({...report,failures:report.failures?.map(f=>({error:f.error}))}));if(!r.ok||report.failures?.length)process.exitCode=1;
