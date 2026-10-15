const {test}=require('node:test');const assert=require('node:assert/strict');const {Inputs,parse}=require('./flow-inputs.js');
test('source precedence, null, reset, and document isolation',()=>{
 const specs=[{name:'base_url',default:'https://httpbingo.org'},{name:'payload',default:null},{name:'required'}],before=JSON.stringify(specs),s=new Inputs(specs);
 assert.throws(()=>s.prepare('http://127.0.0.1:3000'));s.override(s.rows[2],true);assert.equal(s.prepare().required,'');
 assert.equal(s.prepare('http://127.0.0.1:3000').base_url,'http://127.0.0.1:3000');
 s.override(s.rows[1],true);assert.equal(s.rows[1].runtime.source,'null');assert.equal(s.prepare().payload,null);
 s.override(s.rows[0],true,'http://127.0.0.1:3000');assert.equal(s.prepare('https://httpbingo.org').base_url,'http://127.0.0.1:3000');s.override(s.rows[0],false);assert.equal(s.prepare('https://httpbingo.org').base_url,'https://httpbingo.org');assert.equal(JSON.stringify(specs),before);
});
test('strict types and malformed drafts cannot use stale values',()=>{
 const s=new Inputs([{name:'arg',default:'previous'}]),row=s.rows[0];s.override(row,true);s.switchMode(row.runtime,'json');
 for(const source of ['null','false','0','[1,true]','{"a":null}']){row.runtime.source=source;assert.deepEqual(s.prepare().arg,JSON.parse(source));}
 row.runtime.source='{';assert.throws(()=>s.prepare());s.override(row,false);assert.equal(s.prepare().arg,'previous');assert.equal(parse({mode:'text',source:'00123'}),'00123');assert.throws(()=>parse({mode:'json',source:'['.repeat(200)}));
});
test('declaration drafts, duplicate ownership, renaming and row recycling',()=>{
 const s=new Inputs([{name:'a',default:null},{name:'b',default:false}]);s.override(s.rows[0],true);s.rows[0].name='b';s.apply();assert.equal(s.rows[0].applied.name,'a');assert.ok(s.rows[0].error);
 s.rows[1].default.source='0';s.apply();assert.equal(s.rows[1].applied.default,0);
 s.rows[0].name='renamed';s.apply();assert.equal(s.prepare().renamed,null);assert.equal(s.prepare().a,undefined);
 const id=s.rows[0].id;s.remove(id);const row=s.add();row.name='renamed';s.apply();assert.ok(row.id>id);assert.equal(row.overridden,false);assert.throws(()=>s.prepare());
});
test('prototype-like names do not mutate object prototypes',()=>{
 const s=new Inputs([{name:'__proto__',default:{ok:true}},{name:'constructor',default:0}]);const values=s.prepare();assert.deepEqual(values.__proto__,{ok:true});assert.equal(values.constructor,0);assert.equal(Object.getPrototypeOf(values),null);
});
