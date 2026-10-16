const {test}=require('node:test');
const assert=require('node:assert/strict');
const {Files}=require('./flow-files.js');
const flow=()=>({id:'a',document:{flow:{name:'One',inputs:[{name:'token',default:'explicit'}],steps:[]}},runtime:{token:'not-saved'}});
test('save, save as and export maintain distinct document associations',()=>{
 const files=new Files(),a=flow();assert(files.dirty(a));
 assert(files.save(a,'one.http.yml'));assert(!files.dirty(a));
 a.document.flow.name='Two';assert(files.dirty(a));
 assert(files.save(a,'copy.http.yml',{exportCopy:true,openFlows:[a]}));assert(files.dirty(a));assert.equal(a.filePath,'one.http.yml');
 assert(files.save(a,'two.http.yml',{openFlows:[a]}));assert(!files.dirty(a));assert.equal(a.filePath,'two.http.yml');assert.equal(files.open('one.http.yml').flow.name,'One');
 assert.equal(JSON.stringify(files.open('two.http.yml')).includes('not-saved'),false);
 a.document.flow.name='Three';a.document.flow.name='Two';assert(!files.dirty(a));
});
test('pending edits, destination conflicts and open-file collisions preserve snapshots',()=>{
 const files=new Files(),a=flow(),b=flow();b.id='b';files.save(a,'one.http.yml');
 assert.throws(()=>files.save(a,'other.http.yml',{pending:true}));assert(files.dirty(a,true));
 assert.equal(files.save(b,'one.http.yml'),false);assert.equal(b.filePath,undefined);
 assert.throws(()=>files.save(b,'one.http.yml',{replace:true,openFlows:[a,b]}));
 assert.throws(()=>files.save(a,'one.http.yml',{exportCopy:true,openFlows:[a]}));
 const opened=files.open('one.http.yml');opened.flow.name='detached';assert.equal(files.open('one.http.yml').flow.name,'One');
 assert.throws(()=>files.open('missing.http.yml'));
});
