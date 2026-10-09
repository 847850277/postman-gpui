const test = require('node:test');
const assert = require('node:assert/strict');
const M = require('./flow-model.js');

test('both shipped examples pass the local reference checks', () => {
  for (const item of M.samples()) assert.deepEqual(M.validate(item.document), []);
});

test('reordering a consumer before its producer reports the broken reference', () => {
  const doc = M.samples()[0].document;
  [doc.flow.steps[0], doc.flow.steps[1]] = [doc.flow.steps[1], doc.flow.steps[0]];
  assert.ok(M.validate(doc).some(error => error.step === 'get-user' && error.message.includes('create-user.user_id')));
});

test('bindings become explicit DSL expressions and keep typed outputs', () => {
  const expression = M.textExpression('{{base_url}}/users/{{create-user.user_id}}');
  assert.deepEqual(expression, {concat:[{input:'base_url'},{literal:'/users/'},{output:{step:'create-user',name:'user_id'}}]});
  assert.equal(M.resolve(expression,{base_url:'https://example.com'},{'create-user':{user_id:12}}), 'https://example.com/users/12');
  assert.deepEqual(M.resolve({object:{id:M.output('create-user','user_id')}},{},{'create-user':{user_id:12}}), {id:12});
});

test('literal JSON is never mistaken for references', () => {
  const doc = M.samples()[0].document;
  doc.flow.steps[0].request.body = {kind:'json',value:{literal:{output:{step:'missing',name:'data'},input:'not-an-input'}}};
  assert.deepEqual(M.validate(doc), []);
});

test('missing inputs, duplicate exports and invalid polling limits are reported', () => {
  const doc = M.samples()[0].document;
  doc.flow.steps[0].request.url = M.textExpression('https://example.com/{{missing}}');
  doc.flow.steps[0].exports.push({name:'user_id',path:'$.other'});
  const errors = M.validate(doc);
  assert.ok(errors.some(error => error.message.includes('Unknown input')));
  assert.ok(errors.some(error => error.message.includes('unique')));
  const poll = M.samples()[1].document;
  poll.flow.steps[0].max_iterations = 0;
  assert.ok(M.validate(poll).some(error => error.message.includes('Attempts')));
});

test('HTTP conversion preserves configured headers, auth, and an explicit GET body', () => {
  const request = {title:'Example',method:'GET',url:'https://example.com',headers:[{enabled:true,key:'Accept',value:'application/json'},{enabled:false,key:'X-Omit',value:'test'}],authKind:'bearer',token:'sample-token',hasBody:true,body:'',bodyFormat:'raw'};
  const step = M.fromRequest(request,'request-1');
  assert.equal(step.request.headers.length, 2);
  assert.equal(step.request.headers[1].value.literal,'Bearer sample-token');
  assert.deepEqual(step.request.body,{kind:'raw',value:{literal:''}});
  step.request.headers[0].value.literal='changed';
  assert.equal(request.headers[0].value,'application/json');
  assert.throws(() => M.fromRequest({...request,curlOptions:['--insecure']},'request-2'), /transport flags/);
  assert.throws(() => M.fromRequest({...request,method:'PROPFIND'},'request-3'), /does not support/);
});

test('checks distinguish values, missing JSONPaths fail, and skipped outputs can fall back', () => {
  assert.equal(M.condition({eq:[M.literal(1),M.literal('1')]},{},{}), false);
  assert.equal(M.extract({data:[{id:12}]},'$.data[0].id'),12);
  assert.throws(() => M.extract({data:{}},'$.data.missing'), /no value/);
  assert.equal(M.resolve({coalesce:[M.output('skipped','value'),M.literal('fallback')]},{},{}),'fallback');
});
