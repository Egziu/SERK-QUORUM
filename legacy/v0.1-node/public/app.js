'use strict';
const $=id=>document.getElementById(id);
const state={providers:[{id:'openai',name:'ChatGPT',role:'Architect'},{id:'anthropic',name:'Claude',role:'Builder'},{id:'gemini',name:'Gemini',role:'Scientist'},{id:'xai',name:'Grok',role:'Challenger'}],selected:'openai',projects:[{id:'sample',name:'Example product'}],decisions:[],results:[],busy:false};
function el(tag,text,cls){const n=document.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n;}
function projectName(id){return state.projects.find(p=>p.id===id)?.name||'Unknown project';}
function render(){
  $('projectCount').textContent=String(state.projects.length).padStart(2,'0');$('decisionCount').textContent=String(state.decisions.length).padStart(2,'0');
  const old=$('project').value;$('project').replaceChildren(...state.projects.map(p=>{const o=el('option',p.name);o.value=p.id;return o;}));if(state.projects.some(p=>p.id===old))$('project').value=old;
  $('providers').replaceChildren(...state.providers.map(p=>{const b=el('button',undefined,'provider'+(p.id===state.selected?' selected':''));b.type='button';b.disabled=state.busy;b.setAttribute('aria-pressed',String(p.id===state.selected));b.append(el('strong',p.name),el('small',p.role),el('small',p.configured?'Configured · not verified':'Demo available'));b.onclick=()=>{state.selected=p.id;render();};return b;}));
  $('connectionList').replaceChildren(...state.providers.map(p=>el('div',p.name+' — '+(p.configured?'Configured; live test pending':'Not configured'),'record')));
  $('projectList').replaceChildren(...state.projects.map(p=>el('div',p.name,'record')));
  $('decisionList').replaceChildren(...state.decisions.map(d=>el('div',projectName(d.project)+' · '+d.at+'\n'+d.text,'record')));
  $('results').replaceChildren(...state.results.map(r=>{const n=el('article',undefined,'result');n.append(el('h3',state.providers.find(p=>p.id===r.provider)?.name||r.provider),el('small',r.mode==='demo'?'DEMO FIXTURE':r.mode==='live'?'LIVE · '+r.model:'ERROR'),el('pre',r.text));if(r.usage)n.append(el('small','Tokens: '+(r.usage.input??'unknown')+' in / '+(r.usage.output??'unknown')+' out'));return n;}));
  $('submit').disabled=state.busy;$('compare').disabled=state.busy;$('clear').disabled=state.busy;$('live').disabled=state.busy;
}
async function run(ids,live){
  if(state.busy||!$('brief').reportValidity())return;
  if(live&&!window.confirm('Send this brief to the selected external provider? API charges may apply.'))return;
  const prompt=$('prompt').value,task=$('task').value;
  state.busy=true;state.results=[];render();$('status').textContent='Working…';
  for(const provider of ids){
    try{const r=await fetch('/api/run',{method:'POST',headers:{'content-type':'application/json',...(live?{authorization:'Bearer '+$('token').value}:{})},body:JSON.stringify({provider,task,prompt,live})});const data=await r.json();if(!r.ok)throw new Error(data.error||'Request failed');state.results.push(data);}
    catch(e){state.results.push({provider,mode:'error',text:e.message});}render();
  }
  state.busy=false;render();$('status').textContent='Finished. Review each perspective independently.';
}
document.querySelectorAll('.nav').forEach(b=>b.onclick=()=>{document.querySelectorAll('main>section').forEach(s=>s.hidden=s.id!==b.dataset.view);document.querySelectorAll('.nav').forEach(n=>n.classList.toggle('active',n===b));});
$('brief').onsubmit=e=>{e.preventDefault();run([state.selected],$('live').checked);};
$('compare').onclick=()=>run(state.providers.map(p=>p.id),false);
$('clear').onclick=()=>{state.results=[];render();$('status').textContent='';};
$('newProject').onsubmit=e=>{e.preventDefault();const name=$('projectName').value.trim();if(!name)return;state.projects.push({id:crypto.randomUUID(),name});$('projectName').value='';render();$('project').value=state.projects.at(-1).id;};
$('newDecision').onsubmit=e=>{e.preventDefault();const text=$('decision').value.trim();if(!text)return;state.decisions.push({project:$('project').value,text,at:new Date().toISOString()});$('decision').value='';render();};
$('export').onclick=()=>{const blob=new Blob([JSON.stringify({projects:state.projects,decisions:state.decisions},null,2)],{type:'application/json'});const url=URL.createObjectURL(blob);const a=el('a');a.href=url;a.download='quorum-decisions.json';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);};
fetch('/api/status').then(r=>r.json()).then(s=>{state.providers=s.providers;$('connection').textContent=s.liveEnabled?'Live opt-in available':'Local demo';render();}).catch(()=>$('status').textContent='Server unavailable');
render();
