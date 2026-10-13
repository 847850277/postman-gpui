const test = require('node:test');
const assert = require('node:assert/strict');
const demo = require('./httpbingo-demo.js');
const M = require('./flow-model.js');

test('query and form echoes preserve repeated keys as arrays', () => {
  const get = demo.response({url:demo.baseUrl+'/get?tag=one&tag=two&empty='});
  assert.deepEqual(get.data.args,{tag:['one','two'],empty:['']});
  const post = demo.response({url:demo.baseUrl+'/post',method:'POST',bodyFormat:'urlencoded',body:'tag=one&tag=two'});
  assert.equal(post.code,200);
  assert.deepEqual(post.data.form,{tag:['one','two']});
  assert.equal(post.data.json,null);
});

test('HTTPBingo echoes JSON without inventing CRUD status codes or response envelopes', () => {
  const post = demo.response({url:demo.baseUrl+'/post',method:'POST',bodyFormat:'json',body:demo.sampleBody});
  assert.equal(post.status,'200 OK');
  assert.deepEqual(post.data.json,{name:'Maya Chen'});
  assert.equal(post.data.data,JSON.stringify(demo.sampleBody));
  assert.equal(demo.response({url:demo.baseUrl+'/delete',method:'DELETE'}).code,200);
  assert.equal(demo.response({url:demo.baseUrl+'/post',method:'GET'}).code,405);
  assert.deepEqual(demo.response({url:demo.baseUrl+'/status/404'}),{code:404,status:'404 Not Found',data:null});
  assert.equal(demo.response({url:demo.baseUrl+'/status/200'}).data,null);
});

test('both shipped flows execute with HTTPBingo response paths and typed outputs', () => {
  for (const sample of M.samples()) {
    const flow = sample.document.flow;
    const inputs = Object.fromEntries(flow.inputs.map(input => [input.name,input.default]));
    const exports = {};
    function execute(step) {
      if (step.when && !M.condition(step.when,inputs,exports)) return;
      if (step.kind === 'repeat_until') {
        step.steps.forEach(execute);
        assert.equal(M.condition(step.until,inputs,exports),true);
        return;
      }
      const url = M.resolve(step.request.url,inputs,exports);
      assert.equal(new URL(url).origin,demo.baseUrl);
      const response = demo.response({url,method:step.request.method,
        bodyFormat:step.request.body?.kind || 'none',
        body:step.request.body && M.resolve(step.request.body.value,inputs,exports)});
      for (const check of step.checks) assert.equal(response.code,check.equals);
      exports[step.id] = Object.fromEntries(step.exports.map(item => [item.name,M.extract(response.data,item.path)]));
    }
    flow.steps.forEach(execute);
    const outputs = Object.fromEntries(flow.outputs.map(item => [item.name,M.resolve(item.value,inputs,exports)]));
    if (sample.id === 'echo-chain') assert.deepEqual(outputs,{request_id:demo.sampleUuid,status:'active'});
  }
});
