/* Design-reference projection only. Native execution and redaction live in Rust. */
(function(root){
 'use strict';
 const sensitive=name=>/authorization|cookie|session|token|secret|password|credential|api.?key/i.test(name);
 function inspect(value,whole=false,known=[]){
  const secrets=new Set(known.filter(v=>typeof v==='string'&&v));
  const remember=v=>{if(typeof v==='string'&&v)secrets.add(v);else if(v&&typeof v==='object')Object.values(v).forEach(remember);};
  const discover=v=>{if(v&&typeof v==='object')Object.entries(v).forEach(([k,v])=>sensitive(k)?remember(v):discover(v));};
  discover(value);const redacted=[];let count=0;
  const walk=(v,path)=>{
   if(++count>2048)throw new Error('limit');
   if(typeof v==='string'&&[...secrets].some(s=>v.includes(s))){redacted.push(path);return '[REDACTED]';}
   if(Array.isArray(v))return v.map((v,i)=>walk(v,path+'/'+i));
   if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>{
    const p=path+'/'+k.replaceAll('~','~0').replaceAll('/','~1');
    if(sensitive(k)){redacted.push(p);return [k,'[REDACTED]'];}return [k,walk(v,p)];
   }));return v;
  };
  try{
   const safe=whole?(redacted.push(''),'[REDACTED]'):walk(value,'');const json=JSON.stringify(safe,null,2);
   if(json.length>16384)throw new Error('limit');
   return {json,redacted,type:redacted.includes('')?'redacted':safe===null?'null':Array.isArray(safe)?'array':typeof safe};
  }catch{return {json:null,redacted:[],type:'unavailable (size limit)'};}
 }
 const api={inspect};if(typeof module!=='undefined')module.exports=api;else root.FlowResultInspection=api;
})(globalThis);
