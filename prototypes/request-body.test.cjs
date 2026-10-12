const test = require('node:test');
const assert = require('node:assert/strict');
const B = require('./request-body.js');
const C = require('./curl-request.js');
const F = require('./flow-model.js');
const request = (extra = {}) => ({method:'POST',url:'https://example.com/upload',headers:[],body:'{"name":"Maya"}',bodyFormat:'json',bodyExplicit:false,authKind:'none',...extra});

test('None removes the payload on every method without erasing other drafts', () => {
  const r = request(); B.ensure(r); B.select(r,'raw'); r.bodyDrafts.raw = '@literal text';
  B.select(r,'none');
  assert.equal(B.project(r).hasBody,false);
  assert.doesNotMatch(C.stringify(B.project(r)),/--data|Content-Type/);
  B.select(r,'json'); assert.equal(r.body,'{"name":"Maya"}');
  B.select(r,'raw'); assert.equal(r.body,'@literal text');
  assert.equal(B.ensure(request({method:'GET'})).json,'{"name":"Maya"}');
  assert.equal(B.project(request({method:'GET'})).hasBody,false);
});

test('explicit empty raw bodies and GET bodies survive import and export', () => {
  for (const source of ["curl -X GET --data-raw '' https://example.com/", "curl -X GET --json '{}' https://example.com/"]) {
    const parsed = C.parse(source), projected = B.project(parsed);
    assert.equal(projected.hasBody,true);
    assert.equal(C.parse(C.stringify(projected)).body,parsed.body);
  }
});

test('URL encoding keeps duplicates, empty values, Unicode, and disabled drafts', () => {
  const r = request(); B.select(r,'urlencoded');
  r.bodyDrafts.urlencoded = [
    {...B.row(),key:'tag',value:'a & b'}, {...B.row(),key:'tag',value:'中文'},
    {...B.row(),key:'empty'}, {...B.row(),key:'omit',value:'secret',enabled:false},B.row()
  ];
  assert.equal(B.validate(r),null);
  const p = B.project(r);
  assert.equal(p.body,'tag=a+%26+b&tag=%E4%B8%AD%E6%96%87&empty=');
  assert.equal(C.parse(C.stringify(p)).body,p.body);
  const step = F.fromRequest(p,'form');
  assert.equal(step.request.body.value.literal,p.body);
  assert.equal(step.request.headers[0].value.literal,'application/x-www-form-urlencoded');
  B.select(r,'none'); B.select(r,'urlencoded'); assert.equal(r.bodyDrafts.urlencoded[3].value,'secret');
});

test('generated Content-Type follows body kind and preserves explicit overrides', () => {
  const r = request();
  assert.equal(B.headers(r)[0].value,'application/json');
  B.select(r,'raw'); r.bodyDrafts.rawType = 'xml';
  assert.equal(B.headers(r)[0].value,'application/xml');
  r.headers.push({enabled:true,key:'content-type',value:'application/custom+xml'});
  assert.deepEqual(B.headers(r),r.headers);
  r.headers[0].value = ''; assert.equal(B.headerInfo(r).value,'(suppressed)');
  r.headers[0].enabled = false; assert.equal(B.headers(r)[1].value,'application/xml');
  r.headers = []; B.select(r,'multipart'); assert.deepEqual(B.headers(r),[]);
  assert.equal(B.contentType(r),'multipart/form-data');
});

test('invalid JSON, nameless values and missing files identify the editable field', () => {
  const r = request({body:'{'}); assert.equal(B.validate(r).field,'body-editor');
  B.select(r,'urlencoded'); r.bodyDrafts.urlencoded[0].value = 'missing key';
  assert.equal(B.validate(r).field,'body-key-0');
  r.bodyDrafts.urlencoded[0].enabled = false; assert.equal(B.validate(r),null);
  B.select(r,'multipart'); assert.equal(B.validate(r).field,'body-add-field');
  Object.assign(r.bodyDrafts.multipart[0],{key:'avatar',type:'file'});
  assert.equal(B.validate(r).field,'body-file-0');
  B.select(r,'binary'); assert.equal(B.validate(r).field,'body-choose-file');
});

test('multipart export keeps literal text and quotes file paths within form syntax', () => {
  const r = request(); B.select(r,'multipart');
  r.bodyDrafts.multipart = [
    {...B.row(),key:'caption',value:'@literal;type=text/plain'},
    {...B.row(),key:'asset',type:'file',file:{name:'my,"file\\name.txt',type:'text/plain',size:0}},
    {...B.row(),key:'disabled',value:'not exported',enabled:false}
  ];
  assert.equal(B.validate(r),null);
  const code = C.stringify(B.project(r));
  assert.ok(code.includes("--form-string 'caption=@literal;type=text/plain'"));
  assert.ok(code.includes('--form \'asset=@"./my,\\"file\\\\name.txt";type=text/plain\''));
  assert.doesNotMatch(code,/--data-raw|--header|disabled/);
  assert.equal(B.preview(r)[1].file.size,0);
  assert.throws(() => F.fromRequest(B.project(r),'file'),/cannot be added/);
});

test('binary export references a file, including a zero byte file, never its metadata as text', () => {
  const r = request(); B.select(r,'binary');
  r.bodyDrafts.binary = {name:'empty.bin',size:0,type:''};
  assert.equal(B.validate(r),null);
  assert.match(C.stringify(B.project(r)),/--data-binary '@\.\/empty.bin'/);
  assert.equal(B.contentType(r),'application/octet-stream');
  assert.throws(() => F.fromRequest(B.project(r),'file'),/cannot be added/);
});

test('serialized session/history snapshots keep drafts isolated across requests', () => {
  const first = request(); B.select(first,'multipart');
  Object.assign(first.bodyDrafts.multipart[0],{key:'name',value:'Maya'});
  const replay = JSON.parse(JSON.stringify(first));
  replay.bodyDrafts.multipart[0].value = 'Oliver';
  assert.equal(first.bodyDrafts.multipart[0].value,'Maya');
  assert.equal(B.project(replay).multipart[0].value,'Oliver');
});
