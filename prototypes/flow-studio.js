/* Interactive design prototype. Flow execution uses local fixtures only. */
(() => {
 'use strict';
 const $=id=>document.getElementById(id), M=FlowPreview, HTTP=PrototypeHTTP;
 const esc=value=>String(value??'').replace(/[&<>"']/g,ch=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));
 const icon=name=>'<svg class="icon" aria-hidden="true"><use href="#i-'+name+'"/></svg>';
 let sequence=0, activeFlow='onboarding', inspectorTab='request', canvasView='canvas', resultView='steps', mobileInspect=false;
 const flows=M.samples().map(item=>({...item,selected:item.document.flow.steps[0]?.id,dirty:false,run:null,inputs:{}}));
 const current=()=>flows.find(flow=>flow.id===activeFlow);
 const selected=()=>current().document.flow.steps.find(step=>step.id===current().selected);
 const mode=()=>document.body.dataset.view;
 const choice=(value,label,selectedValue)=>'<option value="'+esc(value)+'"'+(value===selectedValue?' selected':'')+'>'+esc(label)+'</option>';
 const field=(label,control)=>'<label class="studio-field"><span>'+label+'</span>'+control+'</label>';
 const input=(id,value,extra='')=>'<input class="studio-input" id="'+id+'" value="'+esc(value)+'" '+extra+'>';
 const textarea=(id,value,extra='')=>'<textarea class="studio-input" id="'+id+'" spellcheck="false" '+extra+'>'+esc(value)+'</textarea>';
 const statuses={running:'Running',passed:'Passed',failed:'Failed',skipped:'Skipped',cancelled:'Cancelled',waiting:'Waiting'};
 function renderHome() {
  $('home-screen').innerHTML='<div class="home-inner"><p class="home-eyebrow">POSTMAN / GPUI</p><h1>One request. Or the whole flow.</h1><p class="home-subtitle">Choose how you want to work. Your requests and flows are always a click away.</p>'+
  '<div class="mode-cards"><button class="mode-card" data-enter="http"><div class="mode-card-top"><div class="mode-card-icon">'+icon('terminal')+'</div><span>01 / REQUEST</span></div><h2>HTTP requests</h2><p>Compose, send, and inspect an API request.<br>Start from a URL or import a cURL command.</p><div class="http-mini" aria-hidden="true"><div class="mini-url"><span class="method">GET</span><span>/v1/users?page=1</span><span>↗</span></div><code class="mini-code"><span class="muted">200 OK · 142 ms</span>\n{ "data": [{ "name": "Maya Chen" }] }</code></div><span class="mode-card-link">Open HTTP editor '+icon('arrow')+'</span></button>'+
  '<button class="mode-card flow-card" data-enter="flows"><div class="mode-card-top"><div class="mode-card-icon">'+icon('flow')+'</div><span>02 / ORCHESTRATE</span></div><h2>Flows</h2><p>Connect requests into a repeatable sequence.<br>Pass variables, check results, and follow every step.</p><div class="flow-mini" aria-hidden="true"><div class="mini-node"><span class="method post">POST</span>Create user</div><span class="mini-line"></span><div class="mini-node"><span class="method">GET</span>Get user</div><span class="mini-line"></span><div class="mini-node"><span class="method patch">PATCH</span>Activate</div></div><span class="mode-card-link">Open flow editor '+icon('arrow')+'</span></button></div>'+
  '<section class="home-recent"><div class="home-section-head"><h2>Continue editing</h2><span>In this session</span></div><div class="recent-columns"><div><div class="recent-group-label">HTTP REQUESTS</div>'+HTTP.list().slice(-3).map(request=>'<button class="recent-item" data-recent-http="'+esc(request.id)+'"><span class="method '+esc(request.method.toLowerCase())+'">'+esc(request.method)+'</span><div><strong>'+esc(request.title)+'</strong><small>'+esc(request.url||'No URL yet')+'</small></div><span>Open</span></button>').join('')+'</div><div><div class="recent-group-label">FLOWS</div>'+flows.slice(-3).map(flow=>'<button class="recent-item" data-recent-flow="'+esc(flow.id)+'">'+icon('flow')+'<div><strong>'+esc(flow.document.flow.name)+'</strong><small>'+flow.document.flow.steps.length+' steps · '+esc(flow.description)+'</small></div><span>Open</span></button>').join('')+'</div></div></section><p class="home-footnote">'+icon('settings')+'Environment settings and appearance are shared across HTTP and Flows.</p></div>';
 }
 $('flow-screen').innerHTML=`
  <header class="flow-topbar"><div><div class="flow-breadcrumb">Flows <span>/</span> <span id="flow-filename"></span><span id="flow-unsaved" hidden>· Edited</span></div><input id="flow-title" class="flow-title-input" aria-label="Flow name" spellcheck="false"></div><div class="flow-top-actions"><label class="environment"><span class="dot"></span><select id="flow-environment" aria-label="Flow environment"></select></label><button class="icon-button" id="flow-settings" title="Manage environments" aria-label="Manage flow environments">${icon('settings')}</button><button class="studio-button" id="export-flow">${icon('import')}Export</button><button class="studio-button" id="check-flow">${icon('check')}Check</button><button class="studio-button primary" id="run-flow">${icon('play')}<span>Run preview</span></button></div></header>
  <div class="flow-layout"><aside class="flow-library" aria-label="Flow library"><div class="library-heading">Your flows<button class="icon-button" id="new-flow" aria-label="New flow" title="New flow">${icon('plus')}</button></div><label class="flow-filter">${icon('search')}<input id="flow-filter" placeholder="Find a flow…" aria-label="Find a flow"></label><div class="flow-list" id="flow-list"></div><div class="library-foot">${icon('code')} .http.yml<br>Visual steps, readable source.<br>Edits stay in this session.</div></aside>
  <section class="flow-work" aria-label="Flow editor"><div class="flow-toolbar"><select class="flow-select-mobile" id="flow-picker" aria-label="Choose flow"></select><div class="view-switch" aria-label="Flow view"><button data-flow-view="canvas" aria-pressed="true">Canvas</button><button data-flow-view="yaml" aria-pressed="false">YAML</button></div><span class="muted" id="flow-step-count"></span><span class="spacer"></span><button class="studio-button quiet" id="flow-inputs">${icon('settings')}Inputs <span class="count" id="flow-input-count"></span></button><button class="studio-button" id="add-step">${icon('plus')}Add step</button><button class="studio-button flow-toolbar-toggle" id="toggle-inspector" aria-pressed="false">Details</button></div>
  <div class="flow-content" id="flow-content"><div class="flow-canvas" id="flow-canvas" aria-label="Ordered flow steps"></div><aside class="flow-inspector" id="flow-inspector" aria-label="Step configuration"></aside><div class="yaml-pane" id="yaml-pane" hidden><div class="yaml-heading"><span>Generated from the canvas · Flow v1</span><button class="studio-button" id="copy-yaml">${icon('copy')}Copy YAML</button></div><pre class="yaml-code" id="yaml-code"></pre></div></div>
  <section class="flow-results" aria-label="Flow run results"><div class="results-heading">${icon('terminal')}<strong>Run results</strong><span class="results-note">Local fixtures</span><span class="spacer"></span><div class="view-switch"><button data-result-view="steps" aria-pressed="true">Steps</button><button data-result-view="outputs" aria-pressed="false">Outputs</button></div><span id="flow-run-status" class="run-status" role="status">Not run</span></div><div id="flow-results-body" class="results-body"></div></section>
  </section></div>`;
 document.body.insertAdjacentHTML('beforeend',`
 <dialog class="studio-dialog" id="new-flow-dialog" aria-labelledby="new-flow-title"><form id="new-flow-form"><div class="dialog-head">${icon('flow')}<strong id="new-flow-title">Create a flow</strong><span class="spacer"></span><button type="button" class="icon-button studio-close" aria-label="Close new flow">${icon('x')}</button></div><div class="dialog-content"><p>Give your sequence a name. Add requests and connect their outputs as you go.</p>${field('Flow name',input('new-flow-name','','required maxlength="80" placeholder="e.g. Order checkout"'))}</div><div class="dialog-footer"><span>Saved in this session</span><button type="button" class="studio-button studio-close">Cancel</button><button class="studio-button primary" type="submit">Create flow</button></div></form></dialog>
 <dialog class="studio-dialog" id="add-step-dialog" aria-labelledby="add-step-title"><div class="dialog-head">${icon('plus')}<strong id="add-step-title">Add a step</strong><span class="spacer"></span><button class="icon-button studio-close" aria-label="Close add step">${icon('x')}</button></div><div class="dialog-content"><button class="recent-item" id="add-blank-step">${icon('terminal')}<div><strong>New HTTP request</strong><small>Configure a method, URL, and response checks</small></div>${icon('arrow')}</button><p class="inspector-section">FROM OPEN HTTP REQUESTS</p><div id="open-request-options"></div></div></dialog>
 <dialog class="studio-dialog" id="add-to-flow-dialog" aria-labelledby="add-to-flow-title"><form id="add-to-flow-form"><div class="dialog-head">${icon('flow')}<strong id="add-to-flow-title">Add to flow</strong><span class="spacer"></span><button type="button" class="icon-button studio-close" aria-label="Close add to flow">${icon('x')}</button></div><div class="dialog-content"><p id="add-to-flow-summary"></p>${field('Destination flow','<select id="destination-flow" class="studio-input"></select>')}<p class="studio-hint">Creates an editable copy at the end of the flow.</p><p class="studio-error" id="transfer-error" role="alert" hidden></p></div><div class="dialog-footer"><span></span><button type="button" class="studio-button studio-close">Cancel</button><button class="studio-button primary" type="submit">Add and open flow</button></div></form></dialog>`);
 let pendingRequest=null;
 function setMode(view,updateHash=true) {
  if(!['home','http','flows'].includes(view))view='home';
  document.body.dataset.view=view;
  $('home-screen').hidden=view!=='home';$('request-editor').hidden=view!=='http';$('flow-screen').hidden=view!=='flows';
  document.querySelectorAll('.mode-button').forEach(button=>{if(button.dataset.view===view)button.setAttribute('aria-current','page');else button.removeAttribute('aria-current');});
  $('mode-label').textContent={home:'Home',http:'HTTP requests',flows:'Flows'}[view];
  const target={home:'home-screen',http:'request-editor',flows:'flow-screen'}[view];
  document.querySelector('.skip-link').href='#'+target;
  if(view==='home')renderHome();if(view==='http')HTTP.refresh();if(view==='flows')renderFlow();
  updateFooter();
  if(updateHash && location.hash!=='#'+view)history.pushState(null,'','#'+view);
 }
 function updateFooter(){if(mode()!=='http')$('footer-status').textContent=mode()==='home'?'Ready when you are':current().dirty?'Flow has unsaved edits · Export to keep':'Flow editor';}
 function switchFlow(id){if(!flows.some(f=>f.id===id))return;activeFlow=id;inspectorTab='request';mobileInspect=false;renderFlow();updateFooter();}
 function syncEnvironment(){const env=HTTP.environment();$('flow-environment').innerHTML=env.items.map(item=>choice(item.id,item.name,env.id)).join('');}
 function filename(){return (current().document.flow.name.toLowerCase().replace(/[^a-z0-9-]+/g,'-').replace(/^-|-$/g,'')||'untitled-flow')+'.http.yml';}
 function renderLibrary(){
  const query=$('flow-filter').value.toLowerCase();
  $('flow-list').innerHTML=flows.filter(flow=>flow.document.flow.name.toLowerCase().includes(query)).map(flow=>'<button class="flow-list-item" data-select-flow="'+esc(flow.id)+'" aria-current="'+(flow.id===activeFlow)+'">'+icon('flow')+'<span><strong>'+esc(flow.document.flow.name)+'</strong><small>'+flow.document.flow.steps.length+' steps'+(flow.dirty?' · Edited':'')+'</small></span></button>').join('')||'<p class="flow-step-list-empty">No matching flows.</p>';
  $('flow-picker').innerHTML=flows.map(flow=>choice(flow.id,flow.document.flow.name,activeFlow)).join('')+choice('__new','+ New flow',activeFlow);
 }
 function renderFlow(){
  const flow=current();$('flow-title').value=flow.document.flow.name;$('flow-filename').textContent=filename();$('flow-unsaved').hidden=!flow.dirty;
  syncEnvironment();renderLibrary();renderCanvas();renderInspector();renderResults();renderView();
  $('flow-step-count').textContent=flow.document.flow.steps.length+' steps · Sequential';$('flow-input-count').textContent=flow.document.flow.inputs.length;
  $('run-flow').querySelector('span').textContent=flow.run?.status==='running'?'Stop':'Run preview';
 }
 function renderCanvas(){
  const flow=current(), steps=flow.document.flow.steps;
  const connector='<div class="chain-connector" aria-hidden="true">'+icon('down')+'</div>';
  $('flow-canvas').innerHTML='<div class="flow-chain"><div class="chain-cap">'+icon('play')+'Flow inputs <span class="count">'+flow.document.flow.inputs.length+'</span></div>'+connector+steps.map((step,index)=>{
   const loop=step.kind==='repeat_until',result=flow.run?.stale?null:flow.run?.steps[step.id],status=result?.status;
   return '<button class="flow-node '+(status||'')+'" data-select-step="'+esc(step.id)+'" aria-pressed="'+(step.id===flow.selected)+'"><div class="node-heading"><span class="node-number">'+String(index+1).padStart(2,'0')+'</span><span class="node-name">'+esc(step.name)+'</span><span class="node-status '+(status||'')+'">'+(status?statuses[status]:loop?'Loop':'HTTP')+'</span></div>'+
    (loop?'<div class="node-url">'+icon('repeat')+'<span>Repeat until healthy · up to '+step.max_iterations+' attempts</span></div><div class="loop-inner"><span class="method">'+esc(step.steps[0].request.method)+'</span>'+esc(step.steps[0].name)+'</div>':'<div class="node-url"><span class="method '+esc(step.request.method.toLowerCase())+'">'+esc(step.request.method)+'</span><span>'+esc(M.expressionLabel(step.request.url))+'</span></div><div class="node-tags">'+(step.checks.length?'<span class="node-tag">'+icon('check')+step.checks.length+' check'+(step.checks.length===1?'':'s')+'</span>':'')+step.exports.map(item=>'<span class="node-tag variable">↗ '+esc(item.name)+'</span>').join('')+'</div>')+
    (step.when?'<div class="node-condition">When '+esc(conditionLabel(step.when))+'</div>':'')+'</button>'+connector;
  }).join('')+'<button class="chain-add" id="canvas-add-step">'+icon('plus')+'Add a step</button>'+connector+'<div class="chain-cap">'+icon('check')+'Flow outputs <span class="count">'+flow.document.flow.outputs.length+'</span></div></div>';
 }
 function conditionLabel(expr){const [operator,values]=Object.entries(expr)[0];return Array.isArray(values)?values.map(value=>Object.hasOwn(value,'literal')?JSON.stringify(value.literal):M.expressionLabel(value)).join(operator==='eq'?' = ':' ≠ '):JSON.stringify(expr);}
 function renderView(){
  const yaml=canvasView==='yaml';$('yaml-pane').hidden=!yaml;$('flow-canvas').hidden=yaml;$('flow-inspector').hidden=yaml;
  $('flow-content').classList.toggle('inspect-mode',mobileInspect);
  $('toggle-inspector').textContent=mobileInspect?'Canvas':'Details';$('toggle-inspector').setAttribute('aria-pressed',mobileInspect);$('toggle-inspector').hidden=yaml;
  document.querySelectorAll('[data-flow-view]').forEach(button=>button.setAttribute('aria-pressed',button.dataset.flowView===canvasView));
  if(yaml)renderYaml();
 }
 function renderYaml(){
  const text=M.yaml(current().document)+'\n';
  $('yaml-code').innerHTML=text.split('\n').map(line=>esc(line).replace(/^(\s*)([\w-]+)(:)/,'$1<span class="yaml-key">$2</span>$3')).join('\n');
 }
 function availableBindings(step){const list=current().document.flow.inputs.map(item=>'{{'+item.name+'}}');for(const item of current().document.flow.steps){if(item.id===step.id)break;for(const exp of item.exports||[])list.push('{{'+item.id+'.'+exp.name+'}}');}return list;}
 function renderInspector(){
  const step=selected(),flow=current();
  $('flow-inspector').scrollTop=0;
  if(inspectorTab==='inputs'){
   const env=HTTP.environment();
   $('flow-inspector').innerHTML='<div class="inspector-heading">'+icon('settings')+'Run inputs</div><div class="inspector-form"><p class="studio-hint" style="margin-top:0;margin-bottom:18px">Values for the next preview run. The selected environment supplies base_url.</p>'+flow.document.flow.inputs.map((item,i)=>field(esc(item.name),input('run-input-'+i,flow.inputs[item.name]??(item.name==='base_url'?env.baseUrl:item.default??''),'data-run-input="'+esc(item.name)+'"'))).join('')+'<p class="studio-hint">Runtime values are separate from the defaults in your YAML.</p></div>';return;
  }
  if(!step){$('flow-inspector').innerHTML='<div class="flow-empty">'+icon('flow')+'Add a request to start your flow.</div>';return;}
  const loop=step.kind==='repeat_until';
  $('flow-inspector').innerHTML='<div class="inspector-heading">'+icon(loop?'repeat':'terminal')+'<span>'+esc(loop?'Loop configuration':'Step configuration')+'</span></div><div class="inspector-tabs">'+['request','checks','exports'].map(tab=>'<button data-inspector-tab="'+tab+'" aria-pressed="'+(tab===inspectorTab)+'">'+{request:loop?'Loop':'Request',checks:'Checks',exports:'Exports'}[tab]+'</button>').join('')+'</div><div class="inspector-form" id="inspector-fields"></div>';
  const area=$('inspector-fields');
  if(inspectorTab==='request'){
   area.innerHTML=field('Step name',input('step-name',step.name))+'<p class="studio-hint mono">'+esc(step.id)+'</p>';
   if(loop){
    area.innerHTML+=field('Maximum attempts',input('loop-limit',step.max_iterations,'type="number" min="1" max="10000"'))+field('Interval (ms)',input('loop-interval',step.interval_ms,'type="number" min="1" max="60000"'))+field('Stop when',textarea('loop-condition',JSON.stringify(step.until,null,2)))+'<p class="inspector-section">REQUEST INSIDE LOOP</p><p class="studio-hint">'+esc(step.steps[0].name)+' · '+esc(M.expressionLabel(step.steps[0].request.url))+'</p>';
   }else{
    area.innerHTML+='<div class="studio-grid">'+field('Method','<select id="step-method" class="studio-input">'+[...new Set(['GET','POST','PUT','PATCH','DELETE','HEAD','OPTIONS',step.request.method])].map(method=>choice(method,method,step.request.method)).join('')+'</select>')+field('URL',input('step-url',M.expressionLabel(step.request.url),'spellcheck="false"'))+'</div>'+
    '<p class="inspector-section">AVAILABLE VARIABLES</p><div>'+availableBindings(step).map(binding=>'<span class="binding">'+esc(binding)+'</span>').join('')+'</div>'+
    '<details><summary class="inspector-section">Headers · '+(step.request.headers||[]).length+'</summary>'+textarea('step-headers',(step.request.headers||[]).map(item=>M.expressionLabel(item.name)+': '+M.expressionLabel(item.value)).join('\n'),'aria-label="Step headers"')+'</details>'+
    '<p class="inspector-section">BODY EXPRESSION</p>'+textarea('step-body',step.request.body?JSON.stringify(step.request.body,null,2):'','aria-label="Step body expression" placeholder="No request body"')+
    '<p class="studio-hint">JSON expressions preserve values from inputs and previous steps.</p>';
   }
   area.innerHTML+='<details'+(step.when?' open':'')+'><summary class="inspector-section">Run condition'+(step.when?' · Enabled':' · Optional')+'</summary>'+textarea('step-when',step.when?JSON.stringify(step.when,null,2):'','aria-label="Step run condition" placeholder="Leave empty to always run"')+'</details>';
  }else if(inspectorTab==='checks'){
   const target=loop?step.steps[0]:step;
   area.innerHTML='<p class="studio-hint" style="margin-top:0;margin-bottom:16px">'+(loop?'Checks run on each polling request.':'A step passes when every response check passes.')+'</p><div class="check-row"><div class="check-row-head">'+icon('check')+'Response status</div>'+field('Expected status code',input('step-status',target.checks.find(check=>check.kind==='status')?.equals??'','type="number" min="100" max="599" placeholder="No status check"'))+'</div><p class="studio-hint">A failed check stops the sequence before its outputs are passed to the next step.</p>';
  }else{
   const target=loop?step.steps[0]:step;
   area.innerHTML='<p class="studio-hint" style="margin-top:0;margin-bottom:16px">'+(loop?'Values from the polling request are available to its stop condition.':'Extract values from the JSON response for later steps.')+'</p><div id="step-export-rows">'+target.exports.map((item,i)=>'<div class="export-row">'+input('export-name-'+i,item.name,'data-export-name="'+i+'" aria-label="Export name '+(i+1)+'" placeholder="Name"')+input('export-path-'+i,item.path,'data-export-path="'+i+'" aria-label="Export path '+(i+1)+'" placeholder="$.data.id"')+'<button class="icon-button" data-remove-export="'+i+'" aria-label="Remove export '+(i+1)+'">'+icon('x')+'</button></div>').join('')+'</div><button class="studio-button quiet" id="add-export">'+icon('plus')+'Add export</button>';
  }
  area.innerHTML+='<p class="studio-error" id="step-error" role="alert" hidden></p><div class="inspector-actions"><button class="icon-button" id="move-step-up" aria-label="Move step up" title="Move step up"'+(flow.document.flow.steps[0]===step?' disabled':'')+'>'+icon('up')+'</button><button class="icon-button" id="move-step-down" aria-label="Move step down" title="Move step down"'+(flow.document.flow.steps.at(-1)===step?' disabled':'')+'>'+icon('down')+'</button><span class="studio-hint" style="margin:0 0 0 3px">Execution order</span><button class="icon-button danger" id="remove-step" aria-label="Remove step" title="Remove step">'+icon('trash')+'</button></div>';
  area.querySelectorAll('input,select,textarea').forEach(control=>control.setAttribute('aria-describedby','step-error'));
  for(const [key,draft] of Object.entries(flow.fieldDrafts||{})) {
   if(draft.step!==step.id)continue;
   const control=$(draft.field);if(control){control.value=draft.value;control.setAttribute('aria-invalid','true');$('step-error').textContent=draft.message;$('step-error').hidden=false;}
  }
  if(flow.run?.status==='running')area.querySelectorAll('input,select,textarea,button').forEach(el=>el.disabled=true);
 }
 function markEdited(){current().dirty=true;current().errors=null;if(current().run&&current().run.status!=='running')current().run.stale=true;$('flow-unsaved').hidden=false;$('flow-filename').textContent=filename();renderCanvas();renderLibrary();if(canvasView==='yaml')renderYaml();renderResults();updateFooter();}
 function stepError(input,message){input.setAttribute('aria-invalid','true');$('step-error').textContent=message;$('step-error').hidden=false;}
 function applyInspector(target){
  const step=selected();if(!step)return;
  const requestStep=step.kind==='repeat_until'?step.steps[0]:step;
  target.removeAttribute('aria-invalid');if($('step-error'))$('step-error').hidden=true;
  const value=target.value,draftKey=step.id+'/'+target.id;
  try{
   if(target.id==='step-name')step.name=value;
   else if(target.id==='step-method')step.request.method=value;
   else if(target.id==='step-url')step.request.url=M.textExpression(value);
   else if(target.id==='step-headers')step.request.headers=value.split('\n').filter(line=>line.trim()).map(line=>{const colon=line.indexOf(':');if(colon<1)throw new Error('Write each header as Name: value.');return {name:M.textExpression(line.slice(0,colon).trim()),value:M.textExpression(line.slice(colon+1).trim())};});
   else if(['step-body','step-when','loop-condition'].includes(target.id)){
    const parsed=value.trim()?JSON.parse(value):null;
    if(parsed!==null&&(typeof parsed!=='object'||Array.isArray(parsed)))throw new Error('Enter a JSON expression object.');
    if(target.id==='step-body'){if(parsed&&!['json','json_template','raw','url_encoded','none'].includes(parsed.kind))throw new Error('Choose a body kind: json, json_template, raw, url_encoded, or none.');if(parsed&&parsed.kind!=='none'&&(!parsed.value||typeof parsed.value!=='object'))throw new Error('The body needs a value expression.');if(parsed)step.request.body=parsed;else delete step.request.body;}
    else if(target.id==='step-when'){if(parsed)step.when=parsed;else delete step.when;}
    else {if(!parsed)throw new Error('A polling loop needs a stop condition.');step.until=parsed;}
   }else if(target.id==='loop-limit')step.max_iterations=Number(value);
   else if(target.id==='loop-interval')step.interval_ms=Number(value);
   else if(target.id==='step-status'){
    if(value&&(!Number.isInteger(Number(value))||Number(value)<100||Number(value)>599))throw new Error('Use a status code from 100 to 599.');
    requestStep.checks=requestStep.checks.filter(check=>check.kind!=='status');if(value)requestStep.checks.unshift({kind:'status',equals:Number(value)});
   }else if(target.dataset.exportName!==undefined)requestStep.exports[Number(target.dataset.exportName)].name=value;
   else if(target.dataset.exportPath!==undefined)requestStep.exports[Number(target.dataset.exportPath)].path=value;
   else return;
   if(current().fieldDrafts)delete current().fieldDrafts[draftKey];
   markEdited();
  }catch(error){
   const message=error instanceof SyntaxError?'Enter valid JSON before leaving this field.':error.message;
   (current().fieldDrafts??={})[draftKey]={step:step.id,field:target.id,value,message};current().dirty=true;
   stepError(target,message);$('flow-unsaved').hidden=false;updateFooter();
  }
 }
 function newFlowDialog(){ $('new-flow-name').value='';$('new-flow-dialog').showModal();$('new-flow-name').focus(); }
 function createFlow(name){const id='flow-'+(++sequence);flows.push({id,description:'Your custom HTTP sequence.',document:{schema_version:1,flow:{name,inputs:[{name:'base_url',default:HTTP.environment().baseUrl}],steps:[],outputs:[]}},selected:null,dirty:true,run:null,inputs:{}});activeFlow=id;return current();}
 function addStep(step){const flow=current();if(flow.run?.status==='running'){HTTP.toast('Stop the preview before changing its steps');return;}flow.document.flow.steps.push(step);flow.selected=step.id;inspectorTab='request';canvasView='canvas';mobileInspect=false;flow.dirty=true;flow.run=null;renderFlow();updateFooter();$('add-step-dialog').close();}
 function openAddStep(){
  if(current().run?.status==='running'){HTTP.toast('Stop the preview before adding a step');return;}
  $('open-request-options').innerHTML=HTTP.list().map(request=>'<button class="recent-item" data-copy-request="'+esc(request.id)+'"><span class="method '+esc(request.method.toLowerCase())+'">'+esc(request.method)+'</span><div><strong>'+esc(request.title)+'</strong><small>'+esc(request.url||'No URL yet')+'</small></div>'+icon('plus')+'</button>').join('');$('add-step-dialog').showModal();
 }
 function openTransfer(){pendingRequest=HTTP.current();$('add-to-flow-summary').textContent=pendingRequest.method+' · '+(pendingRequest.title||pendingRequest.url);$('destination-flow').innerHTML=flows.map(flow=>choice(flow.id,flow.document.flow.name,activeFlow)).join('')+choice('__new','Create a new flow…',activeFlow);$('transfer-error').hidden=true;$('add-to-flow-dialog').showModal();}
 function renderResults(){
  const flow=current(),run=flow.run;
  document.querySelectorAll('[data-result-view]').forEach(button=>button.setAttribute('aria-pressed',button.dataset.resultView===resultView));
  $('flow-run-status').className='run-status '+(run?.status||'');$('flow-run-status').textContent=run?(run.stale?'Previous run · ':'')+statuses[run.status]+(run.status==='passed'?' · '+run.rows.length+' requests':''):'Not run';
  if(flow.errors?.length){$('flow-results-body').innerHTML=flow.errors.map(error=>'<button class="recent-item" data-error-step="'+esc(error.step||'')+'"><span style="color:var(--red)">'+icon('x')+'</span><div><strong>'+esc(error.step||'Flow')+'</strong><small style="white-space:normal;color:var(--red)">'+esc(error.message)+'</small></div></button>').join('');return;}
  if(!run){$('flow-results-body').innerHTML='<div class="results-empty">'+icon('play')+'<div><strong>Follow your flow, one step at a time.</strong>Run a local preview to inspect checks, timing, and outputs.</div></div>';return;}
  if(resultView==='outputs'){$('flow-results-body').innerHTML='<pre class="run-outputs">'+esc(JSON.stringify(run.outputs||{},null,2))+'</pre>';return;}
  $('flow-results-body').innerHTML=run.rows.map(row=>'<div class="run-row">'+icon(row.status==='passed'?'check':row.status==='failed'?'x':'clock')+'<span>'+esc(row.name)+(row.iteration?' <small>· attempt '+row.iteration+'</small>':'')+(row.error?'<br><span style="color:var(--red)">'+esc(row.error)+'</span>':'')+'</span><span class="node-status '+row.status+'">'+(row.code||statuses[row.status])+'</span><small>'+row.elapsed+' ms</small></div>').join('')+(run.error?'<p class="studio-error">'+esc(run.error)+'</p>':'');
  if(!run.rows.length)$('flow-results-body').innerHTML='<p class="studio-hint">'+esc(run.error||'Preparing the first request…')+'</p>';
 }
 function checkFlow(notify=true){
  const draftErrors=Object.values(current().fieldDrafts||{}).map(draft=>({step:draft.step,message:draft.message}));
  const errors=draftErrors.concat(M.validate(current().document));current().errors=errors;renderResults();
  if(errors.length){if(notify)HTTP.toast(errors.length+' issue'+(errors.length>1?'s':'')+' to resolve');return false;}
  if(notify)HTTP.toast('Preview checks passed · '+current().document.flow.steps.length+' steps');return true;
 }
 function repaintRun(flow){if(flow.id!==activeFlow)return;renderCanvas();renderResults();$('run-flow').querySelector('span').textContent=flow.run.status==='running'?'Stop':'Run preview';}
 function cancelRun(flow){if(flow.run?.status!=='running')return;flow.run.cancelled=true;flow.run.wake?.();}
 async function runFlow(){
  const flow=current();if(flow.run?.status==='running'){cancelRun(flow);return;}if(!checkFlow(false))return;
  const snapshot=M.copy(flow.document.flow),inputs=Object.fromEntries(snapshot.inputs.map(item=>[item.name,item.default]));Object.assign(inputs,flow.inputs);if(!Object.hasOwn(flow.inputs,'base_url'))inputs.base_url=HTTP.environment().baseUrl;
  const run={status:'running',steps:{},rows:[],outputs:{},cancelled:false};flow.run=run;flow.errors=[];renderInspector();repaintRun(flow);
  const exports={};
  const wait=ms=>new Promise(resolve=>{const timer=setTimeout(resolve,ms);run.wake=()=>{clearTimeout(timer);resolve();};});
  const assertActive=()=>{if(run.cancelled)throw new Error('Preview cancelled');};
  async function execute(step,iteration){
   assertActive();run.steps[step.id]={status:'running'};repaintRun(flow);
   if(step.when&&!M.condition(step.when,inputs,exports)){run.steps[step.id]={status:'skipped'};run.rows.push({name:step.name,status:'skipped',elapsed:0,iteration});repaintRun(flow);return;}
   if(step.kind==='repeat_until'){
    const start=Date.now();let passed=false;
    for(let index=1;index<=step.max_iterations;index++){
     for(const child of step.steps)await execute(child,index);
     if(M.condition(step.until,inputs,exports)){passed=true;break;}
     if(Date.now()-start>=step.timeout_ms)throw new Error('Polling timed out');
     if(index<step.max_iterations){await wait(Math.min(step.interval_ms,1500));assertActive();}
    }
    if(!passed)throw new Error('Polling reached its attempt limit');run.steps[step.id]={status:'passed'};repaintRun(flow);return;
   }
   const start=Date.now(),address=M.resolve(step.request.url,inputs,exports);if(!/^https?:\/\//.test(address))throw new Error('Invalid request URL');
   await wait(550);assertActive();
   const body=step.request.body?.kind==='json'?M.resolve(step.request.body.value,inputs,exports):{};
   const code=address.includes('/status/404')?404:step.request.method==='POST'?201:step.request.method==='DELETE'?204:200;
   const data=address.includes('/health')?{status:'healthy'}:{data:{id:'usr_25',name:inputs.user_name||'Maya Chen',email:'maya@acme.dev',status:'active',...(body&&typeof body==='object'&&!Array.isArray(body)?body:{})}};
   try{
    for(const check of step.checks){if(check.kind==='status'&&code!==check.equals)throw new Error('Expected status '+check.equals+', received '+code);if(check.kind==='jsonpath'&&JSON.stringify(M.extract(data,check.path))!==JSON.stringify(M.resolve(check.equals,inputs,exports)))throw new Error('Response check failed at '+check.path);}
    const values={};for(const item of step.exports)values[item.name]=M.extract(data,item.path);exports[step.id]=values;
   }catch(error){run.steps[step.id]={status:'failed'};run.rows.push({name:step.name,status:'failed',code,error:error.message,elapsed:Date.now()-start,iteration});throw error;}
   run.steps[step.id]={status:'passed'};run.rows.push({name:step.name,status:'passed',code,elapsed:Date.now()-start,iteration});repaintRun(flow);
  }
  try{
   for(const step of snapshot.steps)await execute(step);
   for(const item of snapshot.outputs)run.outputs[item.name]=M.resolve(item.value,inputs,exports);
   run.status='passed';
  }catch(error){run.status=run.cancelled?'cancelled':'failed';run.error=error.message;run.outputs={};for(const value of Object.values(run.steps))if(value.status==='running')value.status=run.status;}
  delete run.wake;repaintRun(flow);if(flow.id===activeFlow)renderInspector();
 }
 function exportFlow(){
  if(Object.keys(current().fieldDrafts||{}).length){HTTP.toast('Fix the invalid expression before exporting');checkFlow(false);return;}
  const content=M.yaml(current().document)+'\n',blob=new Blob([content],{type:'application/yaml'}),url=URL.createObjectURL(blob),anchor=document.createElement('a');anchor.href=url;anchor.download=filename();anchor.click();setTimeout(()=>URL.revokeObjectURL(url),1000);current().dirty=false;$('flow-unsaved').hidden=true;renderLibrary();updateFooter();HTTP.toast('Flow exported as '+filename());
 }
 document.querySelectorAll('.mode-button').forEach(button=>button.addEventListener('click',()=>setMode(button.dataset.view)));
 $('home-screen').addEventListener('click',event=>{const target=event.target.closest('button');if(!target)return;if(target.dataset.enter)setMode(target.dataset.enter);if(target.dataset.recentHttp){HTTP.activate(target.dataset.recentHttp);setMode('http');}if(target.dataset.recentFlow){switchFlow(target.dataset.recentFlow);setMode('flows');}});
 $('flow-list').addEventListener('click',event=>{const button=event.target.closest('[data-select-flow]');if(button)switchFlow(button.dataset.selectFlow);});
 $('flow-filter').addEventListener('input',renderLibrary);
 $('flow-picker').addEventListener('change',()=>{if($('flow-picker').value==='__new'){renderLibrary();newFlowDialog();}else switchFlow($('flow-picker').value);});
 $('flow-title').addEventListener('input',()=>{current().document.flow.name=$('flow-title').value;markEdited();});
 $('flow-environment').addEventListener('change',()=>{HTTP.setEnvironment($('flow-environment').value);for(const flow of flows)delete flow.inputs.base_url;if(inspectorTab==='inputs')renderInspector();updateFooter();});
 $('flow-settings').addEventListener('click',HTTP.openEnvironments);
 $('environment-dialog').addEventListener('close',()=>{syncEnvironment();if(mode()==='flows'&&inspectorTab==='inputs')renderInspector();updateFooter();});
 document.querySelectorAll('[data-flow-view]').forEach(button=>button.addEventListener('click',()=>{canvasView=button.dataset.flowView;renderView();}));
 document.querySelectorAll('[data-result-view]').forEach(button=>button.addEventListener('click',()=>{resultView=button.dataset.resultView;renderResults();}));
 $('toggle-inspector').addEventListener('click',()=>{mobileInspect=!mobileInspect;renderView();});
 $('flow-inputs').addEventListener('click',()=>{inspectorTab='inputs';canvasView='canvas';mobileInspect=true;renderInspector();renderView();});
 $('flow-canvas').addEventListener('click',event=>{const button=event.target.closest('button');if(!button)return;if(button.id==='canvas-add-step')openAddStep();else if(button.dataset.selectStep){current().selected=button.dataset.selectStep;inspectorTab='request';mobileInspect=true;renderCanvas();renderInspector();renderView();}});
 $('flow-inspector').addEventListener('change',event=>{if(event.target.dataset.runInput!==undefined){current().inputs[event.target.dataset.runInput]=event.target.value;return;}applyInspector(event.target);});
 $('flow-inspector').addEventListener('click',event=>{
  const button=event.target.closest('button');if(!button)return;
  if(button.dataset.inspectorTab){inspectorTab=button.dataset.inspectorTab;renderInspector();return;}
  const flow=current(),step=selected();if(!step||flow.run?.status==='running')return;const list=flow.document.flow.steps,index=list.indexOf(step),target=step.kind==='repeat_until'?step.steps[0]:step;
  if(button.id==='move-step-up'||button.id==='move-step-down'){const next=index+(button.id==='move-step-up'?-1:1);if(next<0||next>=list.length)return;[list[index],list[next]]=[list[next],list[index]];}
  else if(button.id==='remove-step'){for(const [key,draft] of Object.entries(flow.fieldDrafts||{}))if(draft.step===step.id)delete flow.fieldDrafts[key];list.splice(index,1);flow.selected=list[Math.min(index,list.length-1)]?.id;HTTP.toast('Step removed');}
  else if(button.id==='add-export')target.exports.push({name:'value_'+(target.exports.length+1),path:'$.data.id'});
  else if(button.dataset.removeExport!==undefined)target.exports.splice(Number(button.dataset.removeExport),1);else return;
  markEdited();renderInspector();$('flow-step-count').textContent=list.length+' steps · Sequential';
 });
 $('new-flow').addEventListener('click',newFlowDialog);$('add-step').addEventListener('click',openAddStep);
 $('new-flow-form').addEventListener('submit',event=>{event.preventDefault();const name=$('new-flow-name').value.trim();if(!name)return;createFlow(name);$('new-flow-dialog').close();setMode('flows');});
 $('add-blank-step').addEventListener('click',()=>addStep(M.http('request-'+(++sequence),'New request','GET','/v1/users')));
 $('open-request-options').addEventListener('click',event=>{const button=event.target.closest('[data-copy-request]');if(!button)return;const previous=HTTP.current().id;HTTP.activate(button.dataset.copyRequest);const request=HTTP.current();HTTP.activate(previous);if(request.curlOptions?.length){HTTP.toast('Remove cURL transport flags before adding this request to a flow');return;}try{addStep(M.fromRequest(request,'request-'+(++sequence)));}catch(error){HTTP.toast(error.message);}});
 $('add-request-to-flow').addEventListener('click',openTransfer);
 $('add-to-flow-form').addEventListener('submit',event=>{event.preventDefault();const id=$('destination-flow').value;let message='';if(pendingRequest.curlOptions?.length)message='This preview cannot transfer cURL transport flags. Remove them before adding this request.';if(flows.find(flow=>flow.id===id)?.run?.status==='running')message='Stop the destination flow preview before adding a step.';if(message){$('transfer-error').textContent=message;$('transfer-error').hidden=false;return;}let step;try{step=M.fromRequest(pendingRequest,'request-'+(++sequence));}catch(error){$('transfer-error').textContent=error.message;$('transfer-error').hidden=false;return;}if(id==='__new')createFlow(pendingRequest.title+' flow');else activeFlow=id;addStep(step);$('add-to-flow-dialog').close();setMode('flows');});
 $('flow-results-body').addEventListener('click',event=>{const button=event.target.closest('[data-error-step]');if(button?.dataset.errorStep){current().selected=button.dataset.errorStep;inspectorTab='request';canvasView='canvas';mobileInspect=true;renderCanvas();renderInspector();renderView();}});
 $('check-flow').addEventListener('click',()=>checkFlow());$('run-flow').addEventListener('click',runFlow);$('export-flow').addEventListener('click',exportFlow);
 $('copy-yaml').addEventListener('click',async()=>{try{await navigator.clipboard.writeText(M.yaml(current().document)+'\n');HTTP.toast('YAML copied');}catch(_){HTTP.toast('Select the YAML to copy it');}});
 document.querySelectorAll('.studio-close').forEach(button=>button.addEventListener('click',()=>button.closest('dialog').close()));
 document.querySelectorAll('.studio-dialog').forEach(dialog=>dialog.addEventListener('click',event=>{if(event.target!==dialog)return;const r=dialog.getBoundingClientRect();if(event.clientX<r.left||event.clientX>r.right||event.clientY<r.top||event.clientY>r.bottom)dialog.close();}));
 document.addEventListener('keydown',event=>{if(mode()!=='flows'||!(event.metaKey||event.ctrlKey)||document.querySelector('dialog[open]'))return;if(event.key==='Enter'){event.preventDefault();runFlow();}if(event.key.toLowerCase()==='s'){event.preventDefault();exportFlow();}});
 window.addEventListener('popstate',()=>setMode(location.hash.slice(1),false));
 window.addEventListener('hashchange',()=>{if(['home','http','flows'].includes(location.hash.slice(1)))setMode(location.hash.slice(1),false);});
 setMode(location.hash.slice(1)||'home',false);
})();
