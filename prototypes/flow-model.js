/* Editable Flow v1 samples and local preview helpers. No network execution. */
(function(root) {
 'use strict';
 const copy = value => JSON.parse(JSON.stringify(value));
 const literal = value => ({literal:value});
 const output = (step,name) => ({output:{step,name}});
 const url = path => ({concat:[{input:'base_url'},literal(path)]});
 const http = (id,name,method,path,status=200) => ({id,name,request:{kind:'http',method,url:url(path),headers:[{name:literal('Accept'),value:literal('application/json')}]},checks:[{kind:'status',equals:status}],exports:[]});
 function samples() {
  const uuid = http('generate-id','Generate UUID','GET','/uuid');
  uuid.exports=[{name:'request_id',path:'$.uuid'}];
  const echo = http('echo-json','Echo JSON','POST','/post');
  echo.request.headers.push({name:literal('Content-Type'),value:literal('application/json')});
  echo.request.body={kind:'json',value:{object:{id:output('generate-id','request_id'),name:{input:'user_name'}}}};
  echo.exports=[{name:'request_id',path:'$.json.id'},{name:'name',path:'$.json.name'}];
  const patch = http('echo-status','Echo status','PATCH','/patch');
  patch.request.headers.push({name:literal('Content-Type'),value:literal('application/json')});
  patch.request.body={kind:'json',value:{object:{id:output('echo-json','request_id'),status:literal('active')}}};
  patch.when={ne:[output('echo-json','name'),literal('')]};patch.exports=[{name:'status',path:'$.json.status'}];
  const query=http('query-status','Echo ready status','GET','/anything/health?status=ready');query.exports=[{name:'status',path:'$.args.status[0]'}];
  return [
   {id:'echo-chain',description:'Generate an ID and pass it through JSON echo requests.',document:{schema_version:1,flow:{name:'UUID and JSON echo',inputs:[{name:'base_url',default:'https://httpbingo.org'},{name:'user_name',default:'Maya Chen'}],steps:[uuid,echo,patch],outputs:[{name:'request_id',value:output('generate-id','request_id')},{name:'status',value:{coalesce:[output('echo-status','status'),literal('not echoed')]}}]}}},
   {id:'poll-status',description:'Check an echoed value with a bounded polling loop.',document:{schema_version:1,flow:{name:'Poll echoed status',inputs:[{name:'base_url',default:'https://httpbingo.org'}],steps:[{id:'wait-ready',name:'Wait for echoed ready status',kind:'repeat_until',max_iterations:3,interval_ms:1000,timeout_ms:10000,until:{eq:[output('query-status','status'),literal('ready')]},steps:[query]}],outputs:[]}}}
  ];
 }
 function expressionLabel(expr) {
  if (expr === undefined) return '';
  if ('literal' in expr) return typeof expr.literal==='string' ? expr.literal : JSON.stringify(expr.literal);
  if (expr.input) return '{{'+expr.input+'}}';
  if (expr.output) return '{{'+expr.output.step+'.'+expr.output.name+'}}';
  if (expr.concat) return expr.concat.map(expressionLabel).join('');
  return JSON.stringify(expr);
 }
 function textExpression(text) {
  const parts=[];let at=0;
  for (const match of text.matchAll(/\{\{\s*([\w$-]+)(?:\.([\w-]+))?\s*\}\}/g)) {
   if(match.index>at)parts.push(literal(text.slice(at,match.index)));
   parts.push(match[2]?output(match[1],match[2]):{input:match[1]});at=match.index+match[0].length;
  }
  if(at<text.length)parts.push(literal(text.slice(at)));
  return parts.length===1?parts[0]:parts.length?{concat:parts}:literal('');
 }
 function resolve(expr, inputs, exports) {
  if ('literal' in expr) return expr.literal;
  if (expr.input) {if(!Object.hasOwn(inputs,expr.input))throw new Error('Missing input: '+expr.input);return inputs[expr.input];}
  if (expr.output) {const source=exports[expr.output.step];if(!source||!Object.hasOwn(source,expr.output.name))throw new Error('Missing output: '+expr.output.step+'.'+expr.output.name);return source[expr.output.name];}
  if (expr.concat) return expr.concat.map(p=>resolve(p,inputs,exports)).join('');
  if (expr.string) return String(resolve(expr.string,inputs,exports));
  if (expr.array) return expr.array.map(p=>resolve(p,inputs,exports));
  if (expr.object) return Object.fromEntries(Object.entries(expr.object).map(([key,value])=>[key,resolve(value,inputs,exports)]));
  if (expr.coalesce) {for (const item of expr.coalesce) {try{return resolve(item,inputs,exports);}catch(_){}}throw new Error('No available output');}
  throw new Error('This expression is not available in the local preview.');
 }
 function condition(expr,inputs,exports) {
  if(expr.and)return expr.and.every(p=>condition(p,inputs,exports));
  if(expr.or)return expr.or.some(p=>condition(p,inputs,exports));
  if(expr.not)return !condition(expr.not,inputs,exports);
  const [kind,values]=Object.entries(expr)[0], [a,b]=values.map(p=>resolve(p,inputs,exports));
  if(kind==='eq')return JSON.stringify(a)===JSON.stringify(b);
  if(kind==='ne')return JSON.stringify(a)!==JSON.stringify(b);
  throw new Error('This condition is not available in the local preview.');
 }
 function extract(data,path) {
  if(!/^\$(?:\.[\w-]+|\[\d+\])*$/.test(path))throw new Error('Use a JSONPath such as $.json.id.');
  let value=data;
  for(const key of path.slice(1).match(/[\w-]+/g)||[]) {if(value==null||!Object.hasOwn(value,key))throw new Error('Response has no value at '+path);value=value[key];}
  return value;
 }
 function references(value) {
  if(!value||typeof value!=='object')return [];
  if(Object.hasOwn(value,'literal'))return [];
  if(value.output)return [value.output];
  return Object.values(value).flatMap(references);
 }
 function inputReferences(value) {
  if(!value||typeof value!=='object'||Object.hasOwn(value,'literal'))return [];
  if(value.input)return [value.input];
  return Object.values(value).flatMap(inputReferences);
 }
 function validate(document) {
  const errors=[],flow=document.flow,known={},inputs=new Set(flow.inputs.map(item=>item.name));
  if(!flow.name.trim())errors.push({message:'Give this flow a name.'});
  if(!flow.steps.length)errors.push({message:'Add at least one request step.'});
  function checkSteps(steps,scope) {
   const ids=new Set();
   for(const step of steps) {
    const fail=message=>errors.push({step:step.id,message});
    if(!step.id||ids.has(step.id))fail('Step IDs must be unique.');ids.add(step.id);
    if(!step.name.trim())fail('Give the step a name.');
    for(const name of inputReferences({request:step.request,when:step.when}))if(!inputs.has(name))fail('Unknown input: '+name);
    for(const ref of references({request:step.request,when:step.when}))if(!scope[ref.step]?.includes(ref.name))fail('Unresolved reference: '+ref.step+'.'+ref.name+'. Place its source earlier in the flow.');
    const exportNames=new Set();
    for(const item of step.exports||[]){if(exportNames.has(item.name))fail('Export names must be unique.');exportNames.add(item.name);}
    if(step.kind==='repeat_until') {
     if(!Number.isInteger(step.max_iterations)||step.max_iterations<1||step.max_iterations>10000)fail('Attempts must be between 1 and 10,000.');
     if(!Number.isInteger(step.interval_ms)||step.interval_ms<1||step.interval_ms>60000)fail('Interval must be between 1 and 60,000 ms.');
     const childScope={...scope};checkSteps(step.steps,childScope);
     for(const ref of references(step.until))if(!childScope[ref.step]?.includes(ref.name))fail('Unknown polling result: '+ref.step+'.'+ref.name);
    } else {
     if(!['GET','POST','PUT','PATCH','DELETE','HEAD','OPTIONS'].includes(step.request?.method))fail('Choose a method supported by Flow v1.');
     const text=expressionLabel(step.request.url);
     if(!text.trim())fail('Enter a request URL.');
     else {try{const test=new URL(text.replace(/\{\{base_url\}\}/g,'https://preview.invalid').replace(/\{\{[^}]+\}\}/g,'value'));if(!['http:','https:'].includes(test.protocol))throw new Error();}catch(_){fail('Use an HTTP(S) URL or {{base_url}} with a path.');}}
     for(const item of step.exports||[])if(!item.name||!/^\$(?:\.[\w-]+|\[\d+\])*$/.test(item.path))fail('Exports need a name and a JSONPath such as $.json.id.');
    }
    scope[step.id]=(step.exports||[]).map(p=>p.name);
   }
  }
  checkSteps(flow.steps,known);
  for(const ref of references(flow.outputs))if(!known[ref.step]?.includes(ref.name))errors.push({message:'Flow output references an unavailable value: '+ref.step+'.'+ref.name});
  return errors;
 }
 function fromRequest(request,id) {
  try{const url=new URL(request.url);if(!['http:','https:'].includes(url.protocol))throw new Error();}catch(_){throw new Error('Enter an absolute HTTP(S) request URL before adding it to a flow.');}
  if(request.hasBody&&['multipart','binary'].includes(request.bodyFormat))throw new Error('Form-data and binary bodies cannot be added to a flow in this preview. Use JSON, Raw, or URL encoded.');
  if(!['GET','POST','PUT','PATCH','DELETE','HEAD','OPTIONS'].includes(request.method))throw new Error('Flow v1 does not support '+request.method+'. Choose a standard HTTP method first.');
  if(request.curlOptions?.length)throw new Error('Remove cURL transport flags before adding this request to a flow.');
  if(request.headers.some(h=>h.enabled&&h.key&&!h.value&&!h.forceEmpty))throw new Error('cURL header suppression cannot be transferred to a flow in this preview.');
  const step={id,name:request.title||'HTTP request',request:{kind:'http',method:request.method,url:literal(request.url),headers:request.headers.filter(h=>h.enabled&&h.key).map(h=>({name:literal(h.key),value:literal(h.value)}))},checks:[],exports:[]};
  if(request.authKind==='bearer'&&request.token&&!step.request.headers.some(h=>h.name.literal.toLowerCase()==='authorization'))step.request.headers.push({name:literal('Authorization'),value:literal('Bearer '+request.token)});
  if(request.hasBody)step.request.body={kind:request.bodyFormat==='json'?'json_template':'raw',value:literal(request.body)};
  return step;
 }
 function yaml(value,depth=0) {
  const pad='  '.repeat(depth),scalar=v=>JSON.stringify(v);
  if(value===null||typeof value!=='object')return pad+scalar(value);
  if(Array.isArray(value))return value.length?value.map(item=>item!==null&&typeof item==='object'?pad+'-\n'+yaml(item,depth+1):pad+'- '+scalar(item)).join('\n'):pad+'[]';
  return Object.entries(value).filter(([,v])=>v!==undefined).map(([key,v])=>{
   const label=/^[A-Za-z_][\w-]*$/.test(key)?key:scalar(key);
   return pad+label+':'+(v!==null&&typeof v==='object'&&Object.keys(v).length?'\n'+yaml(v,depth+1):' '+scalar(v));
  }).join('\n');
 }
 // Session-only step identities and structural edits; references are deliberately untouched.
 function stepIds(steps) {const ids=new Set();for(const step of steps){ids.add(step.id);for(const id of stepIds(step.steps||[]))ids.add(id);}return ids;}
 function reserveStepIds(flow){const used=flow.usedStepIds??=new Set();for(const id of stepIds(flow.document.flow.steps))used.add(id);return used;}
 function nextStepId(flow){const used=reserveStepIds(flow);let n=flow.document.flow.steps.length+1;while(used.has('request-'+n))n++;const id='request-'+n;used.add(id);return id;}
 function moveStep(flow,id,direction){
  if(flow.run?.status==='running'||![1,-1].includes(direction))return false;
  const steps=flow.document.flow.steps,index=steps.findIndex(s=>s.id===id),next=index+direction;
  if(index<0||next<0||next>=steps.length)return false;
  [steps[index],steps[next]]=[steps[next],steps[index]];return true;
 }
 function removeStep(flow,id){
  if(flow.run?.status==='running')return false;
  const steps=flow.document.flow.steps,index=steps.findIndex(s=>s.id===id);if(index<0)return false;
  reserveStepIds(flow);const ids=stepIds([steps[index]]);steps.splice(index,1);
  const surviving=stepIds(steps);
  for(const [key,draft] of Object.entries(flow.fieldDrafts||{}))if(ids.has(draft.step)&&!surviving.has(draft.step))delete flow.fieldDrafts[key];
  for(const key of Object.keys(flow.document.editor?.nodes||{}))if(ids.has(key)&&!surviving.has(key))delete flow.document.editor.nodes[key];
  if(flow.selected===id)flow.selected=steps[Math.min(index,steps.length-1)]?.id;
  return true;
 }
 const api={stepIds,nextStepId,moveStep,removeStep,copy,literal,output,http,samples,expressionLabel,textExpression,resolve,condition,extract,validate,fromRequest,yaml};
 if(typeof module!=='undefined'&&module.exports)module.exports=api;else root.FlowPreview=api;
})(globalThis);
