/* Interactive design prototype. Flow execution uses local fixtures only. */
(() => {
 'use strict';
 const $=id=>document.getElementById(id), M=FlowPreview, HTTP=PrototypeHTTP;
 const esc=value=>String(value??'').replace(/[&<>"']/g,ch=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));
 const icon=name=>'<svg class="icon" aria-hidden="true"><use href="#i-'+name+'"/></svg>';
 let sequence=0, activeFlow='echo-chain', inspectorTab='request', canvasView='canvas', resultView='logs', mobileInspect=false;
 let pendingRemoval=null;
 let consoleMode='normal', logLevel='all', followLogs=true, eventSequence=0, commandComposing=false;
 const flows=M.samples().map(item=>({...item,selected:item.document.flow.steps[0]?.id,dirty:false,run:null}));
 const emptyFlow={id:null,document:{schema_version:1,flow:{name:'',inputs:[],steps:[],outputs:[]}},dirty:false};
 const files=new FlowFilePreview.Files();
 const current=()=>flows.find(flow=>flow.id===activeFlow)||emptyFlow;
 const selected=()=>current().document.flow.steps.find(step=>step.id===current().selected);
 const mode=()=>document.body.dataset.view;
 const choice=(value,label,selectedValue)=>'<option value="'+esc(value)+'"'+(value===selectedValue?' selected':'')+'>'+esc(label)+'</option>';
 const field=(label,control)=>'<label class="studio-field"><span>'+label+'</span>'+control+'</label>';
 const input=(id,value,extra='')=>'<input class="studio-input" id="'+id+'" value="'+esc(value)+'" '+extra+'>';
 const textarea=(id,value,extra='')=>'<textarea class="studio-input" id="'+id+'" spellcheck="false" '+extra+'>'+esc(value)+'</textarea>';
 const statuses={running:'Running',passed:'Passed',failed:'Failed',skipped:'Skipped',cancelled:'Cancelled',waiting:'Waiting'};
 function renderHome() {
  $('home-screen').innerHTML='<div class="home-inner"><p class="home-eyebrow">POSTMAN / GPUI</p><h1>One request. Or the whole flow.</h1><p class="home-subtitle">Choose how you want to work. Your requests and flows are always a click away.</p>'+
  '<div class="mode-cards"><button class="mode-card" data-enter="http"><div class="mode-card-top"><div class="mode-card-icon">'+icon('terminal')+'</div><span>01 / REQUEST</span></div><h2>HTTP requests</h2><p>Compose, send, and inspect an API request.<br>Start from a URL or import a cURL command.</p><div class="http-mini" aria-hidden="true"><div class="mini-url"><span class="method">GET</span><span>/get?page=1</span><span>↗</span></div><code class="mini-code"><span class="muted">200 OK · 142 ms</span>\n{ "args": { "page": ["1"] } }</code></div><span class="mode-card-link">Open HTTP editor '+icon('arrow')+'</span></button>'+
  '<button class="mode-card flow-card" data-enter="flows"><div class="mode-card-top"><div class="mode-card-icon">'+icon('flow')+'</div><span>02 / ORCHESTRATE</span></div><h2>Flows</h2><p>Connect requests into a repeatable sequence.<br>Pass variables, check results, and follow every step.</p><div class="flow-mini" aria-hidden="true"><div class="mini-node"><span class="method">GET</span>Generate UUID</div><span class="mini-line"></span><div class="mini-node"><span class="method post">POST</span>Echo JSON</div><span class="mini-line"></span><div class="mini-node"><span class="method patch">PATCH</span>Echo status</div></div><span class="mode-card-link">Open flow editor '+icon('arrow')+'</span></button></div>'+
  '<section class="home-recent"><div class="home-section-head"><h2>Continue editing</h2><span>In this session</span></div><div class="recent-columns"><div><div class="recent-group-label">HTTP REQUESTS</div>'+HTTP.list().slice(-3).map(request=>'<button class="recent-item" data-recent-http="'+esc(request.id)+'"><span class="method '+esc(request.method.toLowerCase())+'">'+esc(request.method)+'</span><div><strong>'+esc(request.title)+'</strong><small>'+esc(request.url||'No URL yet')+'</small></div><span>Open</span></button>').join('')+'</div><div><div class="recent-group-label">FLOWS</div>'+flows.slice(-3).map(flow=>'<button class="recent-item" data-recent-flow="'+esc(flow.id)+'">'+icon('flow')+'<div><strong>'+esc(flow.document.flow.name)+'</strong><small>'+flow.document.flow.steps.length+' steps · '+esc(flow.description)+'</small></div><span>Open</span></button>').join('')+'</div></div></section><p class="home-footnote">'+icon('settings')+'Environment settings and appearance are shared across HTTP and Flows.</p></div>';
 }
 $('flow-screen').innerHTML=`
  <header class="flow-topbar"><div><div class="flow-breadcrumb">Flows <span>/</span> <span id="flow-filename"></span><span id="flow-unsaved" hidden>· Edited</span></div><input id="flow-title" class="flow-title-input" aria-label="Flow name" spellcheck="false"></div><div class="flow-top-actions"><label class="environment"><span class="dot"></span><select id="flow-environment" aria-label="Flow environment"></select></label><button class="icon-button" id="flow-settings" title="Manage environments" aria-label="Manage flow environments">${icon('settings')}</button><button class="studio-button" id="open-flow">Open…</button><button class="studio-button" id="save-flow">Save</button><select class="studio-input" id="flow-file-actions" aria-label="Flow file actions"><option value="">⋯</option><option value="save-as">Save as…</option><option value="recent">Recent flows…</option><option value="close">Close flow</option></select><button class="studio-button" id="export-flow">${icon('import')}Export…</button><button class="studio-button" id="check-flow">${icon('check')}Check</button><button class="studio-button primary" id="run-flow">${icon('play')}<span>Run preview</span></button></div></header>
  <div class="flow-layout" id="flow-layout"><aside class="flow-library" id="flow-library" aria-label="Flow library"><div class="library-heading">Your flows<button class="icon-button" id="new-flow" aria-label="New flow" title="New flow">${icon('plus')}</button></div><label class="flow-filter">${icon('search')}<input id="flow-filter" placeholder="Find a flow…" aria-label="Find a flow"></label><div class="flow-list" id="flow-list"></div><div class="library-foot"><button class="studio-button quiet" id="flow-recent-files">Recent flows…</button></div></aside>
  <div class="resize-divider" id="flow-library-divider" aria-controls="flow-library" data-testid="flow-library-divider"><span class="divider-grip" aria-hidden="true"></span></div>
  <section class="flow-work" aria-label="Flow editor"><div class="flow-toolbar"><select class="flow-select-mobile" id="flow-picker" aria-label="Choose flow"></select><div class="view-switch" aria-label="Flow view"><button data-flow-view="canvas" aria-pressed="true">Canvas</button><button data-flow-view="yaml" aria-pressed="false">YAML</button></div><span class="muted" id="flow-step-count"></span><span class="spacer"></span><button class="studio-button quiet" id="flow-inputs">${icon('settings')}Inputs <span class="count" id="flow-input-count"></span></button><button class="studio-button" id="add-step">${icon('plus')}Add step</button><button class="studio-button flow-toolbar-toggle" id="toggle-inspector" aria-pressed="false">Details</button></div>
  <div class="flow-stage" id="flow-stage"><div class="flow-content" id="flow-content"><div class="flow-canvas" id="flow-canvas" aria-label="Ordered flow steps"></div><div class="resize-divider" id="flow-inspector-divider" aria-controls="flow-inspector" data-testid="flow-inspector-divider"><span class="divider-grip" aria-hidden="true"></span></div><aside class="flow-inspector" id="flow-inspector" aria-label="Step configuration"></aside><div class="yaml-pane" id="yaml-pane" hidden><div class="yaml-heading"><span id="yaml-status" role="status">Readonly YAML · Current document · Session draft</span><button class="studio-button" id="copy-yaml">${icon('copy')}Copy YAML</button></div><pre class="yaml-code" id="yaml-code"></pre></div></div>
  <div class="resize-divider" id="flow-results-divider" aria-controls="flow-results" data-testid="flow-results-divider"><span class="divider-grip" aria-hidden="true"></span></div>
  <section class="flow-results" id="flow-results" aria-label="Flow console">
   <div class="results-heading">
    <span class="console-title">${icon('terminal')}<strong>Console</strong></span><span class="results-note">Local preview</span>
    <div class="console-tabs" role="tablist" aria-label="Console view">${['logs','steps','outputs'].map(view=>`<button id="console-tab-${view}" role="tab" data-result-view="${view}" aria-controls="flow-results-panel" aria-selected="${view==='logs'}" tabindex="${view==='logs'?0:-1}">${{logs:'Logs',steps:'Steps',outputs:'Outputs'}[view]}</button>`).join('')}</div>
    <span id="flow-check-status" class="run-status" role="status">Not checked</span><span id="flow-run-status" class="run-status" role="status">Not run</span>
    <button class="icon-button" id="maximize-console" aria-label="Maximize console" title="Maximize console" aria-pressed="false">${icon('expand')}</button>
    <button class="icon-button" id="toggle-console" aria-label="Collapse console" title="Collapse console" aria-controls="flow-results-panel" aria-expanded="true">${icon('down')}</button>
   </div>
   <div id="flow-results-panel" class="console-panel" role="tabpanel" aria-labelledby="console-tab-logs">
    <div class="console-tools">
     <label id="console-level-field"><span class="resize-help">Log level</span><select id="console-level" aria-label="Log level"><option value="all">All levels</option><option value="command">Commands</option><option value="info">Info</option><option value="success">Success</option><option value="warning">Warnings</option><option value="error">Errors</option></select></label>
     <span id="console-count" class="console-count"></span><span class="spacer"></span>
     <button class="console-follow" id="console-follow" aria-pressed="true" title="Follow new log events">${icon('down')}Auto-scroll</button>
     <button class="icon-button" id="copy-console" aria-label="Copy logs" title="Copy logs">${icon('copy')}</button>
     <button class="icon-button" id="clear-console" aria-label="Clear logs" title="Clear logs; keep step results and outputs">${icon('trash')}</button>
    </div>
    <div id="flow-results-body" class="results-body" tabindex="0" aria-label="Console content"></div>
    <form id="console-command-form" class="console-command-form" aria-label="Flow command line">
     <div class="console-command-row"><span class="console-prompt" aria-hidden="true">flow ›</span><input id="console-command" aria-label="Console command" aria-describedby="console-command-hint" placeholder="Type help for commands" autocomplete="off" autocapitalize="off" spellcheck="false" maxlength="240"><button id="submit-console-command" type="submit" class="icon-button" aria-label="Execute command" title="Execute command (Enter)" disabled>${icon('arrow')}</button></div>
     <div class="console-command-footer"><span id="console-command-hint" role="status">Enter to execute · ↑ ↓ history · Tab complete</span><button type="button" id="console-command-help">help</button></div>
    </form>
   </div>
  </section>
  </div></section></div>`;
 document.body.insertAdjacentHTML('beforeend',`
 <dialog class="studio-dialog" id="new-flow-dialog" aria-labelledby="new-flow-title"><form id="new-flow-form"><div class="dialog-head">${icon('flow')}<strong id="new-flow-title">Create a flow</strong><span class="spacer"></span><button type="button" class="icon-button studio-close" aria-label="Close new flow">${icon('x')}</button></div><div class="dialog-content"><p>Give your sequence a name. Add requests and connect their outputs as you go.</p>${field('Flow name',input('new-flow-name','','required maxlength="80" placeholder="e.g. Order checkout"'))}</div><div class="dialog-footer"><span>Saved in this session</span><button type="button" class="studio-button studio-close">Cancel</button><button class="studio-button primary" type="submit">Create flow</button></div></form></dialog>
 <dialog class="studio-dialog" id="remove-step-dialog" aria-labelledby="remove-step-title" aria-describedby="remove-step-description"><div class="dialog-head">${icon('trash')}<strong id="remove-step-title">Remove step?</strong><span class="spacer"></span><button type="button" class="icon-button studio-close" aria-label="Cancel removal">${icon('x')}</button></div><div class="dialog-content"><strong id="remove-step-name"></strong><p class="studio-hint mono" id="remove-step-id"></p><p id="remove-step-description"></p></div><div class="dialog-footer"><span></span><button type="button" class="studio-button studio-close" id="remove-step-cancel" autofocus>Cancel</button><button type="button" class="studio-button danger" id="remove-step-confirm">Remove step</button></div></dialog>
 <dialog class="studio-dialog" id="add-to-flow-dialog" aria-labelledby="add-to-flow-title"><form id="add-to-flow-form"><div class="dialog-head">${icon('flow')}<strong id="add-to-flow-title">Add to flow</strong><span class="spacer"></span><button type="button" class="icon-button studio-close" aria-label="Close add to flow">${icon('x')}</button></div><div class="dialog-content"><div id="transfer-source-field" hidden><button type="button" class="recent-item" id="add-blank-step">${icon('terminal')}<strong>New HTTP request</strong></button>${field('From open HTTP requests','<select id="open-request-options" class="studio-input" aria-label="Open HTTP request"></select>')}</div><p id="add-to-flow-summary"></p><div id="transfer-destination-field">${field('Destination flow','<select id="destination-flow" class="studio-input"></select>')}</div><div id="transfer-name-field" hidden>${field('Flow name',input('transfer-flow-name','','maxlength="80"'))}</div><p class="studio-hint">Adds an independent copy. Your HTTP request stays unchanged.</p><p class="studio-error" id="transfer-error" role="alert" hidden></p></div><div class="dialog-footer"><span></span><button type="button" class="studio-button studio-close">Cancel</button><button class="studio-button primary" id="transfer-submit" type="submit">Add and open flow</button></div></form></dialog>`);
 let pendingRequest=null, pendingTarget=null;
 function setMode(view,updateHash=true) { flows.forEach(f=>f.inputDrafts?.conceal());
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
 function updateFooter(){if(mode()!=='http')$('footer-status').textContent=mode()==='home'?'Ready when you are':current().dirty?'Flow has unsaved edits · Save to keep':'Flow editor';}
 function switchFlow(id){flows.forEach(f=>f.inputDrafts?.conceal());if(!flows.some(f=>f.id===id))return;activeFlow=id;inspectorTab='request';mobileInspect=false;renderFlow();updateFooter();}
 function syncEnvironment(){const env=HTTP.environment();$('flow-environment').innerHTML=env.items.map(item=>choice(item.id,item.name,env.id)).join('');}
 function filename(){return current().filePath||(current().document.flow.name.toLowerCase().replace(/[^a-z0-9-]+/g,'-').replace(/^-|-$/g,'')||'untitled-flow')+'.http.yml';}
 function renderLibrary(){
  const query=$('flow-filter').value.toLowerCase();
  $('flow-list').innerHTML=flows.filter(flow=>flow.document.flow.name.toLowerCase().includes(query)).map(flow=>'<button class="flow-list-item" data-select-flow="'+esc(flow.id)+'" aria-current="'+(flow.id===activeFlow)+'">'+icon('flow')+'<span><strong>'+esc(flow.document.flow.name)+'</strong><small>'+flow.document.flow.steps.length+' steps'+(flow.dirty?' · Edited':'')+'</small></span></button>').join('')||'<p class="flow-step-list-empty">No matching flows.</p>';
  $('flow-picker').innerHTML=flows.map(flow=>choice(flow.id,flow.document.flow.name,activeFlow)).join('')+choice('__new','+ New flow',activeFlow);
 }
 function renderFlow(){
  const flow=current();flow.dirty=!!flow.id&&files.dirty(flow,pendingDocumentEdits(flow));$('flow-title').disabled=!flow.id;['save-flow','export-flow','check-flow','run-flow','add-step','flow-inputs'].forEach(id=>$(id).disabled=!flow.id);$('flow-title').value=flow.document.flow.name;$('flow-filename').textContent=filename();$('flow-unsaved').hidden=!flow.dirty;
  syncEnvironment();renderLibrary();renderCanvas();renderInspector();renderResults();renderView();syncConsoleCommand();
  $('flow-step-count').textContent=flow.document.flow.steps.length+' steps · Sequential';$('flow-input-count').textContent=flow.document.flow.inputs.length;
  $('run-flow').disabled=!flow.id||!!(flow.run?.status==='running'&&flow.run.cancelled);$('run-flow').querySelector('span').textContent=flow.run?.status==='running'?(flow.run.cancelled?'Stopping…':'Cancel'):'Run preview';
 }
 function renderCanvas(){
  const flow=current(), steps=flow.document.flow.steps;
  if(!flow.id){$('flow-canvas').innerHTML='<div class="flow-step-list-empty"><h3>Create or open a Flow</h3><p>Use Open… or New flow to continue.</p></div>';return;}
  const connector='<div class="chain-connector" aria-hidden="true">'+icon('down')+'</div>';
  $('flow-canvas').innerHTML='<div class="flow-chain"><div class="chain-cap">'+icon('play')+'Flow inputs <span class="count">'+flow.document.flow.inputs.length+'</span></div>'+connector+steps.map((step,index)=>{
   const loop=step.kind==='repeat_until',result=flow.run?.stale?null:flow.run?.steps[step.id],status=result?.status;
   return '<button class="flow-node '+(status||'')+'" data-select-step="'+esc(step.id)+'" aria-pressed="'+(step.id===flow.selected)+'"><div class="node-heading"><span class="node-number">'+String(index+1).padStart(2,'0')+'</span><span class="node-name">'+esc(step.name)+'</span><span class="node-status '+(status||'')+'">'+(status?statuses[status]:loop?'Loop':'HTTP')+'</span></div>'+
    (loop?'<div class="node-url">'+icon('repeat')+'<span>Repeat until condition passes · up to '+step.max_iterations+' attempts</span></div><div class="loop-inner"><span class="method">'+esc(step.steps[0].request.method)+'</span>'+esc(step.steps[0].name)+'</div>':'<div class="node-url"><span class="method '+esc(step.request.method.toLowerCase())+'">'+esc(step.request.method)+'</span><span>'+esc(M.expressionLabel(step.request.url))+'</span></div><div class="node-tags">'+(step.checks.length?'<span class="node-tag">'+icon('check')+step.checks.length+' check'+(step.checks.length===1?'':'s')+'</span>':'')+step.exports.map(item=>'<span class="node-tag variable">↗ '+esc(item.name)+'</span>').join('')+'</div>')+
    (step.when?'<div class="node-condition">When '+esc(conditionLabel(step.when))+'</div>':'')+'</button>'+connector;
  }).join('')+'<button class="chain-add" id="canvas-add-step">'+icon('plus')+'Add a step</button>'+connector+'<div class="chain-cap">'+icon('check')+'Flow outputs <span class="count">'+flow.document.flow.outputs.length+'</span></div></div>';
 }
 function conditionLabel(expr){const [operator,values]=Object.entries(expr)[0];return Array.isArray(values)?values.map(value=>Object.hasOwn(value,'literal')?JSON.stringify(value.literal):M.expressionLabel(value)).join(operator==='eq'?' = ':' ≠ '):JSON.stringify(expr);}
 function renderView(){
  const yaml=canvasView==='yaml';$('yaml-pane').hidden=!yaml;$('flow-canvas').hidden=yaml;$('flow-inspector').hidden=yaml;
  $('flow-content').classList.toggle('inspect-mode',mobileInspect);
  $('flow-inspector-divider').hidden=yaml;
  $('toggle-inspector').textContent=mobileInspect?'Canvas':'Details';$('toggle-inspector').setAttribute('aria-pressed',mobileInspect);$('toggle-inspector').hidden=yaml;
  document.querySelectorAll('[data-flow-view]').forEach(button=>button.setAttribute('aria-pressed',button.dataset.flowView===canvasView));
  if(yaml)renderYaml();
 }
 function pendingDocumentEdits(flow){return Object.keys(flow.fieldDrafts||{}).length>0||(flow.inputDrafts?.rows||[]).some(row=>row.error);}
 function checkSignature(flow){return JSON.stringify([flow.document,flow.fieldDrafts||{},(flow.inputDrafts?.rows||[]).map(row=>row.error||null)]);}
 function renderYaml(){
  const flow=current(),pending=pendingDocumentEdits(flow);
  $('copy-yaml').disabled=pending;
  if(!pending)flow.yamlPreview=M.yaml(flow.document)+'\n';
  const text=flow.yamlPreview||'';
  $('yaml-status').textContent=pending?(text?'Last applied document · Stale — fix unapplied edits. Copy is disabled.':'YAML unavailable — fix unapplied document edits. Use Check to locate them.'):'Readonly YAML · Current document · Session draft';
  $('yaml-code').innerHTML=text.split('\n').map(line=>esc(line).replace(/^(\s*)([\w-]+)(:)/,'$1<span class="yaml-key">$2</span>$3')).join('\n');
 }
 function availableBindings(step){const list=current().document.flow.inputs.map(item=>'{{'+item.name+'}}');for(const item of current().document.flow.steps){if(item.id===step.id)break;for(const exp of item.exports||[])list.push('{{'+item.id+'.'+exp.name+'}}');}return list;}
 function renderInspector(){
  const step=selected(),flow=current();
  $('flow-inspector').scrollTop=0;
  $('flow-inspector').classList.toggle('step-details',inspectorTab!=='inputs'&&!!step);
  $('flow-inspector').oninput=$('flow-inspector').onchange=$('flow-inspector').onclick=null;
  if(inspectorTab==='inputs'){
   new FlowInputsPanel($('flow-inspector'),flow,()=>HTTP.environment().baseUrl,()=>{markEdited();$('flow-input-count').textContent=flow.document.flow.inputs.length;},()=>{inspectorTab='request';renderInspector();});return;
  }
  flow.inputDrafts?.conceal();
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
   area.innerHTML='<p class="studio-hint" style="margin-top:0;margin-bottom:16px">'+(loop?'Values from the polling request are available to its stop condition.':'Extract values from the JSON response for later steps.')+'</p><div id="step-export-rows">'+target.exports.map((item,i)=>'<div class="export-row">'+input('export-name-'+i,item.name,'data-export-name="'+i+'" aria-label="Export name '+(i+1)+'" placeholder="Name"')+input('export-path-'+i,item.path,'data-export-path="'+i+'" aria-label="Export path '+(i+1)+'" placeholder="$.json.id"')+'<button class="icon-button" data-remove-export="'+i+'" aria-label="Remove export '+(i+1)+'">'+icon('x')+'</button></div>').join('')+'</div><button class="studio-button quiet" id="add-export">'+icon('plus')+'Add export</button>';
  }
  area.innerHTML+='<p class="studio-error" id="step-error" role="alert" hidden></p>';
  $('flow-inspector').insertAdjacentHTML('beforeend','<div class="step-actions-footer"><div class="inspector-actions"><button class="icon-button" id="move-step-up" aria-label="Move step up" title="Move step up. References stay unchanged; check them after editing the order."'+(flow.document.flow.steps[0]===step?' disabled':'')+'>'+icon('up')+'</button><button class="icon-button" id="move-step-down" aria-label="Move step down" title="Move step down. References stay unchanged; check them after editing the order."'+(flow.document.flow.steps.at(-1)===step?' disabled':'')+'>'+icon('down')+'</button><span class="studio-hint" style="margin:0 0 0 3px">Execution order</span><button class="icon-button danger" id="remove-step" aria-label="Remove step" title="Remove step">'+icon('trash')+'</button></div></div>');
  area.querySelectorAll('input,select,textarea').forEach(control=>control.setAttribute('aria-describedby','step-error'));
  for(const [key,draft] of Object.entries(flow.fieldDrafts||{})) {
   if(draft.step!==step.id)continue;
   const control=$(draft.field);if(control){control.value=draft.value;control.setAttribute('aria-invalid','true');$('step-error').textContent=draft.message;$('step-error').hidden=false;}
  }
  if(flow.run?.status==='running')area.querySelectorAll('input,select,textarea,button').forEach(el=>el.disabled=true);
 }
 function markEdited(){current().dirty=files.dirty(current(),pendingDocumentEdits(current()));current().errors=null;if(current().run&&current().run.status!=='running')current().run.stale=true;$('flow-unsaved').hidden=!current().dirty;$('flow-filename').textContent=filename();renderCanvas();renderLibrary();if(canvasView==='yaml')renderYaml();renderResults();updateFooter();}
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
   stepError(target,message);$('flow-unsaved').hidden=false;renderResults();updateFooter();
  }
 }
 function newFlowDialog(){ $('new-flow-name').value='';$('new-flow-dialog').showModal();$('new-flow-name').focus(); }
 function createFlow(name){const id='flow-'+(++sequence);flows.push({id,description:'Your custom HTTP sequence.',document:{schema_version:1,flow:{name,inputs:[{name:'base_url',default:HTTP.environment().baseUrl}],steps:[],outputs:[]}},selected:null,dirty:true,run:null});activeFlow=id;return current();}
 function addStep(step){const flow=current();if(flow.run?.status==='running'){HTTP.toast('Stop the preview before changing its steps');return;}flow.document.flow.steps.push(step);flow.selected=step.id;inspectorTab='request';canvasView='canvas';mobileInspect=true;flow.dirty=true;flow.run=null;renderFlow();updateFooter();$('add-to-flow-dialog').close();$('step-name')?.focus();}
 function openAddStep(){
  if(current().run?.status==='running'){HTTP.toast('Stop the preview before adding a step');return;}
  openTransfer(null,current().id);
 }
 function transferError(){
  if(!pendingRequest)return 'Choose an open HTTP request.';
  const latest=HTTP.snapshot(pendingRequest.id);
  if(!latest)return 'This HTTP tab was closed. Cancel and choose an open request.';
  if(latest.revision!==pendingRequest.revision)return 'This HTTP request changed. Cancel and reopen Add to flow to copy its latest draft.';
  const id=pendingTarget||$('destination-flow').value;
  if(id==='__new'){
   const name=$('transfer-flow-name').value;
   if(!name.trim())return 'Enter a flow name.';
   if(name.length>80||/[\u0000-\u001f\u007f]/.test(name))return 'Use a single-line name of at most 80 characters.';
  }else{
   const target=flows.find(flow=>flow.id===id);
   if(!target)return 'This flow is no longer available. Choose another flow.';
   if(target.run?.status==='running')return 'Stop the destination flow preview before adding a step.';
  }
  try{M.fromRequest(pendingRequest,'pending-step');}catch(error){return error.message;}
  return '';
 }
 function refreshTransfer(){
  $('transfer-name-field').hidden=!!pendingTarget||$('destination-flow').value!=='__new';
  const message=transferError();$('transfer-error').textContent=message;
  $('transfer-error').hidden=!message||!pendingRequest;$('transfer-submit').disabled=!!message;
  $('add-to-flow-summary').textContent=pendingRequest?pendingRequest.method+' · '+(pendingRequest.title||'HTTP request'):'';
 }
 function openTransfer(source=HTTP.current().id,target=null){
  pendingTarget=target;pendingRequest=source?HTTP.snapshot(source):null;
  $('add-to-flow-title').textContent=target?'Add a step':'Add to flow';
  $('add-to-flow-dialog').querySelector('.dialog-head .studio-close').setAttribute('aria-label',target?'Close add step':'Close add to flow');
  $('transfer-source-field').hidden=!target;$('transfer-destination-field').hidden=!!target;
  $('open-request-options').innerHTML=choice('','Choose a request','')+HTTP.list().map(request=>choice(request.id,request.method+' · '+request.title+' · '+request.id,'')).join('');
  $('destination-flow').innerHTML=flows.map(flow=>choice(flow.id,flow.document.flow.name+' · '+flow.id,activeFlow)).join('')+choice('__new','Create a new flow…',activeFlow);
  $('transfer-flow-name').value=((pendingRequest?.title||'HTTP').slice(0,75)+' flow');
  refreshTransfer();$('add-to-flow-dialog').showModal();
  $(target?'open-request-options':'destination-flow').focus();
 }
 const consoleCommands={
  help:'Show available Flow commands',
  check:'Check the current flow',
  run:'Start a local preview',
  stop:'Cancel the current preview',
  clear:'Clear logs; keep results and command history',
  logs:'Show the event log',
  steps:'Show request results',
  outputs:'Show flow outputs'
 };
 function commandState() { return current().command??={draft:'',history:[],index:0,stash:''}; }
 function commandHint(message='Enter to execute · ↑ ↓ history · Tab complete') { $('console-command-hint').textContent=message; }
 function syncConsoleCommand() {
  $('console-command').value=commandState().draft;
  $('submit-console-command').disabled=!commandState().draft.trim();
  commandHint();
 }
 function rememberCommandDraft() {
  const state=commandState();state.draft=$('console-command').value;
  state.index=state.history.length;state.stash=state.draft;
  $('submit-console-command').disabled=!state.draft.trim();commandHint();
 }
 function clearConsoleLogs() {
  current().logs=[];current().logsCleared=true;renderResults();
 }
 function executeConsoleCommand() {
  if(commandComposing)return;
  const source=$('console-command').value.trim();if(!source)return;
  const flow=current(),state=commandState(),command=source.toLowerCase();
  if(state.history.at(-1)!==source)state.history.push(source);
  if(state.history.length>50)state.history.shift();
  state.index=state.history.length;state.draft='';state.stash='';syncConsoleCommand();
  logLevel='all';$('console-level').value='all';followLogs=true;selectConsoleView('logs');
  if(command==='clear'){clearConsoleLogs();commandHint('Logs cleared. Results and command history kept.');return;}
  if(command==='run'&&flow.run?.status!=='running'){
   // runFlow starts a fresh log, retaining this command as its first event.
   void runFlow(source);return;
  }
  logEvent(flow,'command','› '+source);
  if(!Object.hasOwn(consoleCommands,command)){
   logEvent(flow,'error','Unknown Flow command. Type help for available commands.');commandHint('Unknown command. Type help for available commands.');
  }else if(command==='help'){
   logEvent(flow,'info','Flow commands · local preview\n'+Object.entries(consoleCommands).map(([name,description])=>name.padEnd(9)+description).join('\n'));commandHint('Commands apply to the selected flow.');
  }else if(command==='run')logEvent(flow,'warning','A preview is already running. Use stop to cancel it.');
  else if(command==='stop'){
   if(flow.run?.status==='running')cancelRun(flow);else logEvent(flow,'info','No preview is running.');
  }else if(command==='check'){
   if(flow.run?.status==='running')logEvent(flow,'warning','Stop the preview before checking the flow.');else checkFlow();
  }else if(['logs','steps','outputs'].includes(command)){
   selectConsoleView(command);commandHint('Showing '+command+'.');return;
  }
  renderResults();
 }
 function consoleLogs(flow=current()) { return flow.logs || []; }
 function visibleLogs() { return consoleLogs().filter(row=>logLevel==='all'||row.level===logLevel); }
 function logEvent(flow, level, message, step=null, ownerId=null) {
  const rows=flow.logs??=[];
  rows.push({id:++eventSequence,time:new Date().toISOString(),level,message,step:ownerId||step?.id,name:step?.name});
  // Bound the preview's DOM and memory during long polling runs.
  if(rows.length>500)rows.splice(0,rows.length-500);
  flow.logsCleared=false;
 }
 function logTime(time) { return new Date(time).toLocaleTimeString('en-GB',{hour12:false})+'.'+time.slice(20,23); }
 function locateStep(id) {
  const step=current().document.flow.steps.find(step=>step.id===id||(step.steps||[]).some(child=>child.id===id));
  if(!step)return;
  current().selected=step.id;inspectorTab='request';canvasView='canvas';mobileInspect=true;
  if(consoleMode==='maximized')setConsoleMode('normal');
  renderCanvas();renderInspector();renderView();
  const node=[...$('flow-canvas').querySelectorAll('[data-select-step]')].find(node=>node.dataset.selectStep===step.id);
  node?.scrollIntoView({block:'nearest'});
  (matchMedia('(max-width:800px)').matches?$('step-name'):node)?.focus({preventScroll:true});
 }
 function setConsoleMode(next) {
  consoleMode=next;
  $('flow-stage').classList.toggle('console-collapsed',next==='collapsed');
  $('flow-stage').classList.toggle('console-maximized',next==='maximized');
  $('flow-content').hidden=next==='maximized';
  $('flow-results-divider').hidden=next!=='normal';
  $('flow-results-panel').hidden=next==='collapsed';
  const toggle=$('toggle-console'),maximize=$('maximize-console');
  toggle.setAttribute('aria-expanded',next!=='collapsed');
  toggle.setAttribute('aria-label',next==='collapsed'?'Expand console':'Collapse console');
  toggle.title=toggle.getAttribute('aria-label');toggle.innerHTML=icon(next==='collapsed'?'up':'down');
  maximize.setAttribute('aria-pressed',next==='maximized');
  maximize.setAttribute('aria-label',next==='maximized'?'Restore console size':'Maximize console');
  maximize.title=maximize.getAttribute('aria-label');maximize.innerHTML=icon(next==='maximized'?'restore':'expand');
  resultsResize.refresh();inspectorResize.refresh();
 }
 function selectConsoleView(view) {
  resultView=view;
  if(consoleMode==='collapsed')setConsoleMode('normal');
  renderResults();
 }
 function consoleText() {
  const flow=current(),run=flow.run;
  if(resultView==='logs')return visibleLogs().map(row=>`${row.time} ${row.level.toUpperCase()}${row.name?' ['+row.name+']':''} ${row.message}`).join('\n');
  if(resultView==='outputs')return run?.status==='passed'?JSON.stringify(run.outputs,null,2):'';
  return (flow.errors?.length?flow.errors.map(error=>`${error.step||'Flow'}: ${error.message}`):run?.rows.map(row=>`${row.name} · ${statuses[row.status]}${row.code?' · HTTP '+row.code:''} · ${row.elapsed} ms${row.error?' · '+row.error:''}`)||[]).join('\n');
 }
 function renderResults(){
  const flow=current(),run=flow.run,body=$('flow-results-body'),scroll=body.scrollTop;
  const focused=document.activeElement?.dataset.consoleEvent;
  document.querySelectorAll('[data-result-view]').forEach(button=>{
   const active=button.dataset.resultView===resultView;
   button.setAttribute('aria-selected',active);button.tabIndex=active?0:-1;
  });
  $('flow-results-panel').setAttribute('aria-labelledby','console-tab-'+resultView);
  const errors=flow.errors||[];
  $('flow-check-status').textContent=!flow.checkResult?'Not checked':flow.checkResult.signature!==checkSignature(flow)?'Check stale':flow.checkResult.passed?'Check passed':'Check failed';
  const status=run?.status||'';
  $('flow-run-status').className='run-status '+status;
  $('flow-run-status').textContent=run?(run.stale?'Previous run · ':'')+statuses[run.status]+(run.elapsed!==undefined?' · '+(run.elapsed/1000).toFixed(2)+' s':''):'Not run';
  const logs=resultView==='logs';
  $('console-level-field').hidden=!logs;$('console-follow').hidden=!logs;$('clear-console').hidden=!logs;
  $('console-follow').setAttribute('aria-pressed',followLogs);
  $('clear-console').disabled=!consoleLogs().length;
  $('copy-console').disabled=!consoleText();
  $('copy-console').setAttribute('aria-label','Copy '+resultView);$('copy-console').title='Copy '+resultView;
  $('console-count').textContent=logs?`${logLevel==='all'?'':visibleLogs().length+' of '}${consoleLogs().length} events`:resultView==='steps'?`${run?.rows.length||0} completed request${run?.rows.length===1?'':'s'}`:'Flow outputs';
  const empty=(title,text)=>'<div class="results-empty">'+icon('terminal')+'<div><strong>'+esc(title)+'</strong>'+esc(text)+'</div></div>';
  if(logs){
   const rows=visibleLogs();
   body.innerHTML=rows.length?rows.map(row=>'<div class="console-row"><time datetime="'+row.time+'">'+logTime(row.time)+'</time><span class="console-level '+row.level+'">'+row.level.toUpperCase()+'</span><div class="console-message">'+(row.step?'<button class="console-step" data-error-step="'+esc(row.step)+'" data-console-event="'+row.id+'" title="Show step '+esc(row.name)+'">'+esc(row.name)+icon('arrow')+'</button>':'')+'<span>'+esc(row.message)+'</span></div></div>').join(''):empty(flow.logsCleared?'Logs cleared':consoleLogs().length?'No matching events':'Ready to follow your flow',flow.logsCleared?'New events will appear here. Step results and outputs are kept.':consoleLogs().length?'Choose another log level to see more events.':'Run preview to see requests, checks, and polling events. No network requests are sent.');
  }else if(resultView==='outputs'){
   body.innerHTML=run?.status==='passed'?'<pre class="run-outputs">'+esc(JSON.stringify(run.outputs||{},null,2))+'</pre>':empty('No outputs yet',run?.status==='running'?'Outputs appear when the preview completes.':'Complete a successful preview to inspect flow outputs.');
  }else if(errors.length){
   body.innerHTML=errors.map(error=>'<button class="recent-item" data-error-step="'+esc(error.step||'')+'"><span class="console-level error">'+icon('x')+'</span><div><strong>'+esc(error.step||'Flow')+'</strong><small class="console-error">'+esc(error.message)+'</small></div></button>').join('');
  }else if(run?.rows.length){
   body.innerHTML=run.rows.map(row=>'<button class="run-row '+row.status+'" data-error-step="'+esc(row.step)+'">'+icon(row.status==='passed'?'check':row.status==='failed'?'x':'clock')+'<span>'+esc(row.name)+(row.iteration?' <small>· attempt '+row.iteration+'</small>':'')+(row.error?'<br><span class="console-error">'+esc(row.error)+'</span>':'')+'</span><span class="node-status '+row.status+'">'+(row.code||statuses[row.status])+'</span><small>'+row.elapsed+' ms</small></button>').join('')+(run.error?'<p class="studio-error">'+esc(run.error)+'</p>':'');
  }else body.innerHTML=empty(run?.status==='running'?'Running first request…':'No step results yet',run?.error||'Run preview to inspect each request and its checks.');
  if(focused)body.querySelector('[data-console-event="'+focused+'"]')?.focus({preventScroll:true});
  body.scrollTop=logs&&followLogs?body.scrollHeight:scroll;
 }
 function checkFlow(notify=true){
  const flow=current();
  const draftErrors=Object.values(flow.fieldDrafts||{}).map(draft=>({step:draft.step,message:draft.message}));
  for(const row of flow.inputDrafts?.rows||[])if(row.error)draftErrors.push({message:'Input definition: '+row.error});
  const errors=draftErrors.concat(M.validate(flow.document));flow.errors=errors;
  flow.checkResult={signature:checkSignature(flow),passed:errors.length===0};
  if(errors.length){
   for(const error of errors){logEvent(flow,'error',error.message,{id:error.step,name:error.step||'Flow'});flow.logs.at(-1).checkSignature=flow.checkResult.signature;}
   selectConsoleView('logs');
   if(notify)HTTP.toast(errors.length+' issue'+(errors.length>1?'s':'')+' to resolve');return false;
  }
  if(notify){logEvent(flow,'success','Preview checks passed · '+flow.document.flow.steps.length+' steps');selectConsoleView('logs');HTTP.toast('Preview checks passed');}
  else renderResults();
  return true;
 }
 function repaintRun(flow){if(flow.id!==activeFlow)return;renderCanvas();renderResults();$('run-flow').disabled=!!(flow.run.status==='running'&&flow.run.cancelled);$('run-flow').querySelector('span').textContent=flow.run.status==='running'?(flow.run.cancelled?'Stopping…':'Cancel'):'Run preview';}
 function cancelRun(flow){if(flow.run?.status!=='running'||flow.run.cancelled)return;logEvent(flow,'warning','Cancellation requested');flow.run.cancelled=true;flow.run.wake?.();repaintRun(flow);}
 async function runFlow(command=null){
  const flow=current();if(flow.run?.status==='running'){cancelRun(flow);return;}
  const commandLine=typeof command==='string'?command:null;
  if(commandLine)logEvent(flow,'command','› '+commandLine);
  if(!checkFlow(false))return;
  const snapshot=M.copy(flow.document.flow);let inputs;try{inputs=(flow.inputDrafts??=new FlowInputDrafts.Inputs(snapshot.inputs)).prepare(HTTP.environment().baseUrl);}catch(error){logEvent(flow,'error',error.message);inspectorTab='inputs';mobileInspect=true;renderInspector();renderView();renderResults();return;}
  const run={status:'running',steps:{},rows:[],outputs:{},cancelled:false,started:Date.now()};flow.run=run;flow.errors=[];flow.logs=[];
  if(commandLine)logEvent(flow,'command','› '+commandLine);
  logEvent(flow,'info','Preview started · local fixtures only');selectConsoleView('logs');renderInspector();repaintRun(flow);
  const exports={};let executing=null;
  const wait=ms=>new Promise(resolve=>{const timer=setTimeout(resolve,ms);run.wake=()=>{clearTimeout(timer);resolve();};});
  const assertActive=()=>{if(run.cancelled)throw new Error('Preview cancelled');};
  async function execute(step,iteration,ownerId=step.id){
   assertActive();executing={step,ownerId};run.steps[step.id]={status:'running'};
   logEvent(flow,'info',step.kind==='repeat_until'?'Polling started':'Request started'+(iteration?' · attempt '+iteration:''),step,ownerId);repaintRun(flow);
   if(step.when&&!M.condition(step.when,inputs,exports)){run.steps[step.id]={status:'skipped'};run.rows.push({step:ownerId,name:step.name,status:'skipped',elapsed:0,iteration});logEvent(flow,'warning','Skipped · run condition was false',step,ownerId);repaintRun(flow);return;}
   if(step.kind==='repeat_until'){
    const start=Date.now();let passed=false;
    for(let index=1;index<=step.max_iterations;index++){
     for(const child of step.steps)await execute(child,index,ownerId);
     if(M.condition(step.until,inputs,exports)){passed=true;break;}
     if(Date.now()-start>=step.timeout_ms)throw new Error('Polling timed out');
     if(index<step.max_iterations){logEvent(flow,'info','Waiting before attempt '+(index+1),step,ownerId);repaintRun(flow);await wait(Math.min(step.interval_ms,1500));assertActive();}
    }
    if(!passed)throw new Error('Polling reached its attempt limit');run.steps[step.id]={status:'passed'};logEvent(flow,'success','Polling condition passed',step,ownerId);repaintRun(flow);return;
   }
   const start=Date.now(),address=M.resolve(step.request.url,inputs,exports);if(!/^https?:\/\//.test(address))throw new Error('Invalid request URL');
   await wait(550);assertActive();
   const payload=step.request.body,headers=step.request.headers.map(h=>({key:M.resolve(h.name,inputs,exports),value:M.resolve(h.value,inputs,exports)}));
   let body=payload?M.resolve(payload.value,inputs,exports):undefined;
   const contentType=headers.find(h=>h.key.toLowerCase()==='content-type')?.value;
   const bodyFormat=!payload?'none':payload.kind==='json'||payload.kind==='json_template'?'json':contentType==='application/x-www-form-urlencoded'?'urlencoded':'raw';
   if(payload?.kind==='json_template')body=JSON.parse(body);
   const {code,data}=HTTPBingoDemo.response({url:address,method:step.request.method,headers,body,bodyFormat});
   logEvent(flow,'info',step.request.method+' · HTTP '+code+' · '+(Date.now()-start)+' ms',step,ownerId);
   try{
    for(const check of step.checks){if(check.kind==='status'&&code!==check.equals)throw new Error('Expected status '+check.equals+', received '+code);if(check.kind==='jsonpath'&&JSON.stringify(M.extract(data,check.path))!==JSON.stringify(M.resolve(check.equals,inputs,exports)))throw new Error('Response check failed at '+check.path);}
    const values={};for(const item of step.exports)values[item.name]=M.extract(data,item.path);exports[step.id]=values;
   }catch(error){run.steps[step.id]={status:'failed'};run.rows.push({step:ownerId,name:step.name,status:'failed',code,error:error.message,elapsed:Date.now()-start,iteration});logEvent(flow,'error',error.message,step,ownerId);throw error;}
   run.steps[step.id]={status:'passed'};run.rows.push({step:ownerId,name:step.name,status:'passed',code,elapsed:Date.now()-start,iteration});logEvent(flow,'success',step.checks.length+' check'+(step.checks.length===1?'':'s')+' passed · '+step.exports.length+' value'+(step.exports.length===1?'':'s')+' exported',step,ownerId);repaintRun(flow);
  }
  try{
   for(const step of snapshot.steps)await execute(step);
   for(const item of snapshot.outputs)run.outputs[item.name]=M.resolve(item.value,inputs,exports);
   run.status='passed';logEvent(flow,'success','Preview completed · '+run.rows.length+' request'+(run.rows.length===1?'':'s'));
  }catch(error){run.status=run.cancelled?'cancelled':'failed';run.error=error.message;logEvent(flow,run.cancelled?'warning':'error',(run.cancelled?'':'Preview failed · ')+error.message,executing?.step,executing?.ownerId);run.outputs={};for(const value of Object.values(run.steps))if(value.status==='running')value.status=run.status;}
  run.elapsed=Date.now()-run.started;delete run.wake;repaintRun(flow);if(flow.id===activeFlow)renderInspector();
 }
 function downloadFlow(path){
  const content=M.yaml(current().document)+'\n',blob=new Blob([content],{type:'application/yaml'}),url=URL.createObjectURL(blob),anchor=document.createElement('a');anchor.href=url;anchor.download=path;anchor.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
 }
 const fileUI=FlowFilePreview.mount({files,flows,current,filename,pending:()=>pendingDocumentEdits(current()),download:downloadFlow,refresh:()=>{renderFlow();updateFooter();},
  open:(path,document)=>{const opened=flows.find(f=>f.filePath===path);if(opened){switchFlow(opened.id);return;}const flow=createFlow(document.flow.name);flow.document=document;flow.selected=document.flow.steps[0]?.id;flow.filePath=path;flow.savedSource=JSON.stringify(document);flow.dirty=false;},
  stop:async()=>{const flow=current();cancelRun(flow);while(flow.run?.status==='running')await new Promise(resolve=>setTimeout(resolve,10));},
  close:()=>{const index=flows.findIndex(f=>f.id===activeFlow);if(index<0)return;flows.splice(index,1);activeFlow=flows[Math.min(index,flows.length-1)]?.id||null;renderFlow();updateFooter();}
 });
 function exportFlow(){fileUI.export();}
 document.querySelectorAll('.mode-button').forEach(button=>button.addEventListener('click',()=>setMode(button.dataset.view)));
 $('home-screen').addEventListener('click',event=>{const target=event.target.closest('button');if(!target)return;if(target.dataset.enter)setMode(target.dataset.enter);if(target.dataset.recentHttp){HTTP.activate(target.dataset.recentHttp);setMode('http');}if(target.dataset.recentFlow){switchFlow(target.dataset.recentFlow);setMode('flows');}});
 $('flow-list').addEventListener('click',event=>{const button=event.target.closest('[data-select-flow]');if(button)switchFlow(button.dataset.selectFlow);});
 $('flow-filter').addEventListener('input',renderLibrary);
 $('flow-picker').addEventListener('change',()=>{if($('flow-picker').value==='__new'){renderLibrary();newFlowDialog();}else switchFlow($('flow-picker').value);});
 $('flow-title').addEventListener('input',()=>{current().document.flow.name=$('flow-title').value;markEdited();});
 $('flow-environment').addEventListener('change',()=>{HTTP.setEnvironment($('flow-environment').value);if(inspectorTab==='inputs')renderInspector();updateFooter();});
 $('flow-settings').addEventListener('click',HTTP.openEnvironments);
 $('environment-dialog').addEventListener('close',()=>{syncEnvironment();if(mode()==='flows'&&inspectorTab==='inputs')renderInspector();updateFooter();});
 document.querySelectorAll('[data-flow-view]').forEach(button=>button.addEventListener('click',()=>{canvasView=button.dataset.flowView;renderView();}));
 document.querySelectorAll('[data-result-view]').forEach(button=>button.addEventListener('click',()=>selectConsoleView(button.dataset.resultView)));
 $('flow-results').querySelector('[role="tablist"]').addEventListener('keydown',event=>{
  const views=['logs','steps','outputs'],index=views.indexOf(resultView);
  const next={ArrowLeft:(index+2)%3,ArrowRight:(index+1)%3,Home:0,End:2}[event.key];
  if(next===undefined)return;event.preventDefault();selectConsoleView(views[next]);$('console-tab-'+views[next]).focus();
 });
 $('console-command-form').addEventListener('submit',event=>{event.preventDefault();executeConsoleCommand();$('console-command').focus({preventScroll:true});});
 $('console-command-help').addEventListener('click',()=>{$('console-command').value='help';executeConsoleCommand();$('console-command').focus({preventScroll:true});});
 $('console-command').addEventListener('input',rememberCommandDraft);
 $('console-command').addEventListener('compositionstart',()=>commandComposing=true);
 $('console-command').addEventListener('compositionend',()=>{commandComposing=false;rememberCommandDraft();});
 $('console-command').addEventListener('keydown',event=>{
  if(event.isComposing||commandComposing||event.keyCode===229){event.stopPropagation();return;}
  const state=commandState(),input=$('console-command');
  if(event.key==='Enter'){event.preventDefault();event.stopPropagation();$('console-command-form').requestSubmit();return;}
  if(event.metaKey||event.ctrlKey){if(event.key.toLowerCase()==='s')event.preventDefault();event.stopPropagation();return;}
  if(event.key==='Escape'){event.preventDefault();event.stopPropagation();input.value='';rememberCommandDraft();return;}
  if(['ArrowUp','ArrowDown'].includes(event.key)&&state.history.length){
   event.preventDefault();event.stopPropagation();
   if(state.index===state.history.length)state.stash=input.value;
   state.index=Math.max(0,Math.min(state.history.length,state.index+(event.key==='ArrowUp'?-1:1)));
   state.draft=state.index===state.history.length?state.stash:state.history[state.index];
   input.value=state.draft;input.setSelectionRange(input.value.length,input.value.length);
   $('submit-console-command').disabled=!state.draft.trim();return;
  }
  if(event.key==='Tab'&&!event.shiftKey&&input.value.trim()){
   const prefix=input.value.trim().toLowerCase(),matches=Object.keys(consoleCommands).filter(name=>name.startsWith(prefix));
   if(matches.length===1&&matches[0]!==prefix){event.preventDefault();input.value=matches[0];rememberCommandDraft();commandHint('Completed '+matches[0]+'. Enter to execute; Tab to leave.');}
   else if(matches.length>1)commandHint('Matches: '+matches.join(', ')+'. Type more to complete.');
  }
 });
 $('console-level').addEventListener('change',()=>{logLevel=$('console-level').value;renderResults();});
 $('console-follow').addEventListener('click',()=>{followLogs=!followLogs;renderResults();});
 $('clear-console').addEventListener('click',()=>{clearConsoleLogs();HTTP.toast('Logs cleared; step results and outputs kept');});
 $('copy-console').addEventListener('click',async()=>{try{await navigator.clipboard.writeText(consoleText());HTTP.toast('Console '+resultView+' copied');}catch(_){HTTP.toast('Select the console text to copy it');}});
 $('toggle-console').addEventListener('click',()=>setConsoleMode(consoleMode==='collapsed'?'normal':'collapsed'));
 $('maximize-console').addEventListener('click',()=>setConsoleMode(consoleMode==='maximized'?'normal':'maximized'));
 $('flow-results-body').addEventListener('scroll',()=>{
  const body=$('flow-results-body');
  if(resultView==='logs'&&followLogs&&body.scrollTop+body.clientHeight<body.scrollHeight-4){followLogs=false;$('console-follow').setAttribute('aria-pressed','false');}
 });
 $('toggle-inspector').addEventListener('click',()=>{mobileInspect=!mobileInspect;renderView();});
 $('flow-inputs').addEventListener('click',()=>{inspectorTab='inputs';canvasView='canvas';mobileInspect=true;renderInspector();renderView();});
 $('flow-canvas').addEventListener('click',event=>{const button=event.target.closest('button');if(!button)return;if(button.id==='canvas-add-step')openAddStep();else if(button.dataset.selectStep){current().selected=button.dataset.selectStep;inspectorTab='request';mobileInspect=true;renderCanvas();renderInspector();renderView();}});
 $('flow-inspector').addEventListener('input',event=>{if(inspectorTab!=='inputs'&&!event.isComposing)applyInspector(event.target);});
 $('flow-inspector').addEventListener('compositionend',event=>{if(inspectorTab!=='inputs')applyInspector(event.target);});
 $('flow-inspector').addEventListener('change',event=>{if(inspectorTab==='inputs')return;applyInspector(event.target);});
 $('flow-inspector').addEventListener('click',event=>{
  const button=event.target.closest('button');if(!button)return;
  if(button.dataset.inspectorTab){inspectorTab=button.dataset.inspectorTab;renderInspector();return;}
  const flow=current(),step=selected();if(!step||flow.run?.status==='running')return;const list=flow.document.flow.steps,target=step.kind==='repeat_until'?step.steps[0]:step;
  if(button.id==='move-step-up'||button.id==='move-step-down'){if(!M.moveStep(flow,step.id,button.id==='move-step-up'?-1:1))return;}
  else if(button.id==='remove-step'){
   pendingRemoval={flow:flow.id,step:step.id};$('remove-step-name').textContent=step.name||'Untitled step';$('remove-step-id').textContent=step.id;
   $('remove-step-description').textContent=step.steps?'This step, its contained steps, and their unfinished edits will be removed. References to them will remain unchanged.':'This step and its unfinished edits will be removed. References to it will remain unchanged.';
   $('remove-step-dialog').showModal();$('remove-step-cancel').focus();return;
  }
  else if(button.id==='add-export')target.exports.push({name:'value_'+(target.exports.length+1),path:'$.json.id'});
  else if(button.dataset.removeExport!==undefined)target.exports.splice(Number(button.dataset.removeExport),1);else return;
  markEdited();renderInspector();$('flow-step-count').textContent=list.length+' steps · Sequential';
  if(button.id.startsWith('move-step-')){const action=$(button.id);(action?.disabled?$(button.id==='move-step-up'?'move-step-down':'move-step-up'):action)?.focus();}
 });
 $('remove-step-dialog').addEventListener('close',()=>{pendingRemoval=null;});
 $('remove-step-confirm').addEventListener('click',()=>{
  const target=pendingRemoval;pendingRemoval=null;const flow=current();
  const removed=target&&flow.id===target.flow&&M.removeStep(flow,target.step);
  $('remove-step-dialog').close();if(!removed)return;
  markEdited();renderFlow();updateFooter();HTTP.toast('Step removed');
  (flow.selected?$('remove-step'):$('add-step'))?.focus();
 });
 $('new-flow').addEventListener('click',newFlowDialog);$('add-step').addEventListener('click',openAddStep);
 $('new-flow-form').addEventListener('submit',event=>{event.preventDefault();const name=$('new-flow-name').value.trim();if(!name)return;createFlow(name);$('new-flow-dialog').close();setMode('flows');});
 $('add-blank-step').addEventListener('click',()=>addStep(M.http(M.nextStepId(current()),'New request','GET','/get')));
 $('open-request-options').addEventListener('change',()=>{pendingRequest=HTTP.snapshot($('open-request-options').value);refreshTransfer();});
 $('destination-flow').addEventListener('change',refreshTransfer);
 $('transfer-flow-name').addEventListener('input',refreshTransfer);
 $('add-request-to-flow').addEventListener('click',()=>openTransfer());
 $('add-to-flow-dialog').addEventListener('close',()=>{pendingRequest=null;pendingTarget=null;});
 $('add-to-flow-form').addEventListener('submit',event=>{
  event.preventDefault();if(transferError()){refreshTransfer();return;}
  const step=M.fromRequest(pendingRequest,'pending-step'),id=pendingTarget||$('destination-flow').value;
  if(id==='__new')createFlow($('transfer-flow-name').value.trim());else activeFlow=id;
  step.id=M.nextStepId(current());addStep(step);setMode('flows');$('step-name')?.focus();
 });
 $('flow-results-body').addEventListener('click',event=>{const button=event.target.closest('[data-error-step]');if(button?.dataset.errorStep){const row=consoleLogs().find(row=>String(row.id)===button.dataset.consoleEvent);if(row?.checkSignature&&row.checkSignature!==checkSignature(current())){HTTP.toast('This diagnostic is stale. Check the current flow again.');return;}locateStep(button.dataset.errorStep);}});
 $('check-flow').addEventListener('click',()=>checkFlow());$('run-flow').addEventListener('click',runFlow);$('export-flow').addEventListener('click',exportFlow);
 $('copy-yaml').addEventListener('click',async()=>{if(pendingDocumentEdits(current()))return;renderYaml();try{await navigator.clipboard.writeText(current().yamlPreview);HTTP.toast('YAML copied');}catch(_){HTTP.toast('Select the YAML to copy it');}});
 document.querySelectorAll('.studio-close').forEach(button=>button.addEventListener('click',()=>button.closest('dialog').close()));
 document.querySelectorAll('.studio-dialog').forEach(dialog=>dialog.addEventListener('click',event=>{if(event.target!==dialog)return;const r=dialog.getBoundingClientRect();if(event.clientX<r.left||event.clientX>r.right||event.clientY<r.top||event.clientY>r.bottom)dialog.close();}));
 document.addEventListener('keydown',event=>{if(mode()!=='flows'||event.isComposing||!(event.metaKey||event.ctrlKey)||document.querySelector('dialog[open]'))return;if(event.key==='Enter'&&!event.target.closest('#console-command-form,.input-fields')){event.preventDefault();runFlow();}if(event.key.toLowerCase()==='s'){event.preventDefault();fileUI.save(event.shiftKey);}if(event.key.toLowerCase()==='o'){event.preventDefault();fileUI.open();}if(event.key.toLowerCase()==='w'){event.preventDefault();fileUI.close();}});
 window.addEventListener('popstate',()=>setMode(location.hash.slice(1),false));
 window.addEventListener('hashchange',()=>{if(['home','http','flows'].includes(location.hash.slice(1)))setMode(location.hash.slice(1),false);});
 PanelResize.attach({
  container:$('flow-layout'),handle:$('flow-library-divider'),axis:'x',key:'flow-library',label:'Flow library width',initial:196,
  limits:total=>[168,Math.min(340,total-620)],
  apply:value=>$('flow-layout').style.setProperty('--flow-library-width',value+'px')
 });
 const inspectorResize=PanelResize.attach({
  container:$('flow-content'),handle:$('flow-inspector-divider'),axis:'x',key:'flow-inspector',label:'Step details width',initial:300,fromEnd:true,
  limits:total=>[260,total-300],
  apply:value=>$('flow-content').style.setProperty('--flow-inspector-width',value+'px')
 });
 new MutationObserver(()=>inspectorResize.refresh()).observe($('flow-inspector-divider'),{attributes:true,attributeFilter:['hidden']});
 const resultsResize=PanelResize.attach({
  container:$('flow-stage'),handle:$('flow-results-divider'),axis:'y',key:'flow-results',label:'Console height',initial:240,fromEnd:true,
  limits:total=>{
   const chrome=$('flow-results').querySelector('.results-heading').offsetHeight+$('flow-results').querySelector('.console-tools').offsetHeight+$('console-command-form').offsetHeight;
   return [Math.min(chrome+48,total*.5),total-Math.min(220,total*.5)];
  },
  apply:value=>$('flow-stage').style.setProperty('--flow-results-height',value+'px')
 });
 setMode(location.hash.slice(1)||'home',false);
})();
