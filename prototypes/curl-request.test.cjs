const test = require('node:test');
const assert = require('node:assert/strict');
const {parse, stringify} = require('./curl-request.js');
const header = (request, name) => request.headers.find(h => h.key.toLowerCase() === name.toLowerCase())?.value;
const quote = value => "'" + value.replace(/'/g, "'\\''") + "'";

test('a minimal GET has no sample headers, token, or body', () => {
  const r = parse("curl 'https://example.com/users?page=1&limit=10'");
  assert.equal(r.method, 'GET');
  assert.equal(r.url, 'https://example.com/users?page=1&limit=10');
  assert.deepEqual(r.headers, []);
  assert.equal(r.authKind, 'none');
  assert.equal(r.token, '');
  assert.equal(r.body, '');
  assert.equal(r.hasBody, false);
});

test('multiline JSON, bearer auth, cookies, and duplicate headers survive export/import', () => {
  const body = JSON.stringify({name:"O'Connor", text:'line 1\n"quoted" \\ path', template:'$HOME `literal`'},null,2);
  const r = parse("curl -X PATCH 'https://example.com/users/1?tag=a&tag=b' \\\n" +
    " -H 'Content-Type: application/json' -H 'Authorization: Bearer test-token' \\\n" +
    " -H 'X-Tag: one' -H 'X-Tag: two' -b 'session=123' --data-raw " + quote(body));
  assert.equal(r.method, 'PATCH');
  assert.equal(r.body, body);
  assert.equal(r.bodyFormat, 'json');
  assert.equal(r.authKind, 'bearer');
  assert.equal(r.token, 'test-token');
  assert.equal(header(r,'cookie'), 'session=123');
  assert.deepEqual(r.headers.filter(h => h.key === 'X-Tag').map(h => h.value), ['one','two']);
  assert.deepEqual(parse(stringify(r)), r);
});

test('double quotes and CRLF continuation preserve JSON escapes and escaped shell characters', () => {
  const r = parse('curl "https://example.com" \\\r\n --data-raw "{\\"path\\":\\"C:\\\\temp\\",\\"key\\":\\"\\$literal\\"}"');
  assert.equal(r.body, '{"path":"C:\\temp","key":"$literal"}');
});

test('compact flags, equals arguments, and persistent transport options are retained', () => {
  const r = parse("curl -sSLkg -XPATCH --url=https://example.com -H'X-Test: yes' --compressed --data-raw=value");
  assert.equal(r.method, 'PATCH');
  assert.equal(header(r, 'x-test'), 'yes');
  assert.deepEqual(r.curlOptions, ['--location','--insecure','--globoff','--compressed']);
  assert.deepEqual(parse(stringify(r)), r);
});

test('data implies POST, while explicit GET and empty bodies survive a round trip', () => {
  assert.equal(parse('curl https://example.com -d a=1').method, 'POST');
  for (const command of ["curl https://example.com -XGET --data-raw ''", "curl https://example.com -XDELETE --data-raw '@literal'"]) {
    const r = parse(command);
    assert.equal(r.hasBody, true);
    assert.deepEqual(parse(stringify(r)), r);
  }
});

test('repeated form fields and --get move encoded values into query parameters', () => {
  const r = parse("curl -G 'https://example.com/find?existing=1#section' --data-urlencode 'name=Maya Chen' --data-urlencode 'tag=a&b'");
  assert.equal(r.url, 'https://example.com/find?existing=1&name=Maya%20Chen&tag=a%26b#section');
  assert.equal(r.method, 'GET');
  assert.equal(r.hasBody, false);
  assert.equal(r.body, '');
  assert.equal(parse("curl https://example.com --data-urlencode '=hello world'").body, 'hello%20world');
  assert.equal(parse("curl https://example.com -d a=1 -d b=2").body, 'a=1&b=2');
});

test('--json adds implicit headers and concatenates fragments without changing explicit headers', () => {
  const r = parse(`curl https://example.com --json '{"a":' --json '1}' -H 'Accept: text/plain'`);
  assert.equal(r.body, '{"a":1}');
  assert.equal(header(r,'Content-Type'), 'application/json');
  assert.equal(header(r,'Accept'), 'text/plain');
  assert.equal(r.bodyFormat, 'json');
  assert.deepEqual(parse(stringify(r)), r);
});

test('Basic and OAuth tokens import without overriding explicit Authorization', () => {
  const basic = parse("curl https://example.com -u '用户:päss'");
  assert.equal(header(basic,'Authorization'), 'Basic ' + Buffer.from('用户:päss').toString('base64'));
  assert.equal(basic.authKind, 'none');
  const oauth = parse('curl https://example.com --oauth2-bearer test-token');
  assert.equal(oauth.authKind, 'bearer');
  assert.equal(oauth.token, 'test-token');
  const explicit = parse("curl https://example.com -u name:password -H 'Authorization: Custom abc'");
  assert.equal(header(explicit,'Authorization'), 'Custom abc');
  assert.equal(explicit.authKind, 'none');
});

test('empty header suppression, forced empty headers, and duplicate Authorization are preserved', () => {
  const r = parse("curl https://example.com -H 'Content-Type:' -H 'X-Empty;' -H 'Authorization: Bearer a' -H 'Authorization: Bearer b' -d text");
  assert.equal(header(r,'Content-Type'), '');
  assert.equal(r.headers.find(h => h.key === 'X-Empty').forceEmpty, true);
  assert.equal(r.authKind, 'none');
  assert.deepEqual(parse(stringify(r)), r);
});

test('disabled headers are not exported and an explicit auth header takes precedence', () => {
  const r = parse("curl https://example.com -H 'X-Test: value' -H 'Authorization: Custom abc'");
  r.headers[0].enabled = false;
  r.authKind = 'bearer'; r.token = 'ignored';
  const exported = parse(stringify(r));
  assert.equal(header(exported,'X-Test'), undefined);
  assert.equal(header(exported,'Authorization'), 'Custom abc');
  assert.equal(exported.token, '');
});

test('HEAD, custom methods, executable paths, and the end-of-options marker', () => {
  assert.equal(parse('/usr/bin/curl -I https://example.com').method, 'HEAD');
  assert.equal(parse('curl.exe -X PROPFIND -- https://example.com').method, 'PROPFIND');
});

test('invalid or unsupported commands fail instead of silently dropping request settings', () => {
  const cases = [
    ['', /Start with/],
    ['wget https://example.com', /Start with/],
    ['curl', /Add an HTTP/],
    ['curl example.com', /complete URL/],
    ['curl ftp://example.com', /Only HTTP/],
    ['curl https://one.example https://two.example', /one URL/],
    ["curl 'https://example.com", /quotation/],
    ['curl https://example.com \\', /backslash/],
    ['curl https://example.com -H', /Add a value/],
    ["curl https://example.com -H 'Bad header'", /Write headers/],
    ["curl https://example.com -H 'X-Test: first\nsecond'", /single-line/],
    ['curl https://example.com --connect-timeout 10', /Unsupported option/],
    ['curl https://example.com -F file=@file.txt', /Unsupported option/],
    ['curl https://example.com --data-binary @file.json', /File-based/],
    ['curl https://example.com --data-urlencode name@file.txt', /File-based/],
    ['curl https://example.com -H @headers.txt', /Header files/],
    ['curl https://example.com -b cookies.txt', /Cookie files/],
    ['curl https://example.com -u username', /username:password/],
    ["curl https://example.com --json '{}' -d a=1", /either --json/],
    ['curl https://example.com -I -d a=1', /--head/],
    ['curl https://example.com | sh', /single cURL/],
    ['curl https://example.com; echo hello', /single cURL/],
    ['curl https://example.com/$TOKEN', /resolved values/],
    ['curl https://example.com -d "$(id)"', /resolved values/],
    ["curl https://example.com --data-raw 'a\0b'", /null character/],
  ];
  for (const [command, error] of cases) assert.throws(() => parse(command), error, command);
});
