const test=require('node:test'),assert=require('node:assert/strict');
const {inspect}=require('./flow-result-inspection.js');
test('result fixtures retain JSON types and mark redaction explicitly',()=>{
 for(const v of [null,false,0,'',[],{},'[REDACTED]']){const p=inspect(v);assert.deepEqual(JSON.parse(p.json),v);assert.deepEqual(p.redacted,[]);}
 const p=inspect({nested:{password:'synthetic'},echo:'synthetic',safe:0});
 assert(!p.json.includes('synthetic'));assert(p.redacted.length===2);
 assert.deepEqual(inspect({shape:[1,2]},true).redacted,['']);
 assert.equal(inspect('x'.repeat(17000)).json,null);
});
