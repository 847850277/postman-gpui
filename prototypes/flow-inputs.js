/* Inputs design state. Definitions and session overrides have independent ownership. */
(function(root) {
 'use strict';
 const own=(o,k)=>Object.prototype.hasOwnProperty.call(o,k);
 const copy=v=>JSON.parse(JSON.stringify(v));
 const value=v=>({mode:typeof v==='string'?'text':'json',source:typeof v==='string'?v:JSON.stringify(v,null,2)});
 function parse(v) {
  if(new TextEncoder().encode(v.source).length>2*1024*1024)throw Error('Value exceeds 2 MiB.');
  if(v.mode==='text')return v.source;
  try { const result=JSON.parse(v.source), stack=[[result,0]];while(stack.length){const [x,depth]=stack.pop();if(depth>127)throw Error();if(x&&typeof x==='object')for(const child of Object.values(x))stack.push([child,depth+1]);}return result; }
  catch { throw Error('Enter a valid JSON value.'); }
 }
 class Inputs {
  constructor(specs) { this.next=0;this.rows=specs.map(spec=>this.row(spec));this.definitions=false; }
  row(spec) { return {id:this.next++,name:spec?.name||'',hasDefault:!!spec&&own(spec,'default'),sensitive:!!spec?.sensitive,default:value(spec&&own(spec,'default')?spec.default:''),runtime:value(''),overridden:false,applied:spec?copy(spec):null,error:null,revealed:false}; }
  add(){const row=this.row(null);this.rows.push(row);this.definitions=true;this.apply();return row;}
  remove(id){this.rows=this.rows.filter(r=>r.id!==id);return this.apply();}
  conceal(){this.rows.forEach(r=>r.revealed=false);}
  apply(){
   const candidates=this.rows.map(r=>{try{if(!r.name.trim())throw Error('Name cannot be empty.');return {value:{name:r.name,...(r.hasDefault?{default:parse(r.default)}:{}),...(r.sensitive?{sensitive:true}:{})}};}catch(e){return {error:e.message};}});
   while(true){const names=new Map();candidates.forEach((c,i)=>{const v=c.value||this.rows[i].applied;if(v)names.set(v.name,[...(names.get(v.name)||[]),i]);});const bad=[];
    for(const [name,rows] of names){if(rows.length<2)continue;const owners=rows.filter(i=>this.rows[i].applied?.name===name),owner=owners.length===1?owners[0]:-1;for(const i of rows)if(i!==owner&&candidates[i].value)bad.push(i);}
    if(!bad.length)break;bad.forEach(i=>candidates[i]={error:'Input name is already declared.'});
   }
   candidates.forEach((c,i)=>{this.rows[i].error=c.error||null;if(c.value)this.rows[i].applied=c.value;});
   return this.rows.flatMap(r=>r.applied?[copy(r.applied)]:[]);
  }
  inherited(row,base){const spec=row.applied;if(!spec)return {source:'Incomplete definition'};if(spec.name==='base_url'&&base!=null)return {source:'Environment',value:base};if(own(spec,'default'))return {source:'Default',value:spec.default};return {source:'Required / missing'};}
  override(row,enabled,base){if(enabled){const inherited=this.inherited(row,base);row.runtime=value(own(inherited,'value')?inherited.value:'');}row.overridden=enabled;row.revealed=false;}
  switchMode(editor,mode){if(editor.mode===mode)return;const v=parse(editor);if(mode==='text'&&typeof v!=='string')throw Error('Only JSON strings can switch to Text without changing their meaning.');editor.source=mode==='text'?v:JSON.stringify(v,null,2);editor.mode=mode;}
  prepare(base){const result=Object.create(null);for(const row of this.rows){if(row.error||!row.applied)throw Error('Correct input definitions before preparing a run.');const inherited=this.inherited(row,base);if(row.overridden)result[row.applied.name]=parse(row.runtime);else if(own(inherited,'value'))result[row.applied.name]=copy(inherited.value);else throw Error('A required input is missing.');}return result;}
 }
 const api={Inputs,parse};if(typeof module!=='undefined')module.exports=api;else root.FlowInputDrafts=api;
})(typeof window!=='undefined'?window:globalThis);
