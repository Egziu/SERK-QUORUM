import http from 'node:http';
import {readFile} from 'node:fs/promises';
import {timingSafeEqual} from 'node:crypto';
import {PROVIDERS,validate,demo,publicProviders,configured} from './lib/workspace.mjs';
import {runLive} from './lib/providers.mjs';

const assets={'/':['index.html','text/html; charset=utf-8'],'/app.js':['app.js','text/javascript; charset=utf-8'],'/style.css':['style.css','text/css; charset=utf-8'],'/logo.svg':['logo.svg','image/svg+xml']};
function equal(a,b){const x=Buffer.from(a||''),y=Buffer.from(b||'');return x.length===y.length&&x.length>0&&timingSafeEqual(x,y);}
function json(res,status,value){res.writeHead(status,{'content-type':'application/json; charset=utf-8'});res.end(JSON.stringify(value));}
async function body(req){let size=0;const parts=[];for await(const part of req){size+=part.length;if(size>20000)throw new Error('Request too large');parts.push(part);}return JSON.parse(Buffer.concat(parts).toString('utf8'));}
export function createServer(env=process.env,liveRunner=runLive){
  let active=false,lastLive=0;
  return http.createServer(async(req,res)=>{
    res.setHeader('Content-Security-Policy',"default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'");
    res.setHeader('X-Content-Type-Options','nosniff');res.setHeader('Referrer-Policy','no-referrer');res.setHeader('Cache-Control','no-store');
    const path=req.url?.split('?')[0];
    try{
      if(req.method==='GET'&&Object.hasOwn(assets,path)){
        const [file,type]=assets[path];res.writeHead(200,{'content-type':type});res.end(await readFile(new URL('./public/'+file,import.meta.url)));return;
      }
      if(req.method==='GET'&&path==='/api/status'){return json(res,200,{version:'0.1.0',providers:publicProviders(env),liveEnabled:env.QUORUM_LIVE==='1'&&env.QUORUM_ACCESS_TOKEN?.length>=32});}
      if(req.method!=='POST'||path!=='/api/run')return json(res,404,{error:'Not found'});
      if(!req.headers['content-type']?.startsWith('application/json'))return json(res,415,{error:'JSON required'});
      let input;try{input=validate(await body(req));}catch{return json(res,400,{error:'Invalid request. Check task, provider and brief length.'});}
      if(!input.live)return json(res,200,demo(input));
      if(env.QUORUM_LIVE!=='1'||!env.QUORUM_ACCESS_TOKEN||env.QUORUM_ACCESS_TOKEN.length<32)return json(res,403,{error:'Live requests disabled'});
      if(!equal(req.headers.authorization,'Bearer '+env.QUORUM_ACCESS_TOKEN))return json(res,401,{error:'Operator authorization required'});
      if(!configured(PROVIDERS.find(p=>p.id===input.provider),env))return json(res,409,{error:'Provider not configured'});
      if(active||Date.now()-lastLive<5000)return json(res,429,{error:'One live request at a time; wait five seconds'});
      active=true;lastLive=Date.now();
      try{return json(res,200,await liveRunner(input,env));}catch{return json(res,502,{error:'Provider unavailable or returned no text'});}finally{active=false;}
    }catch{if(!res.headersSent)json(res,500,{error:'Unexpected server error'});else res.end();}
  });
}
if(process.argv[1]&&new URL(import.meta.url).pathname===process.argv[1]){
  const host=process.env.HOST||'127.0.0.1';
  if(host!=='127.0.0.1'&&host!=='::1')throw new Error('Preview supports loopback only; use an authenticated private tunnel for remote testing.');
  const server=createServer();server.requestTimeout=60000;server.headersTimeout=10000;
  server.listen(Number(process.env.PORT||8080),host,()=>console.log('QUORUM preview listening on '+host));
}
