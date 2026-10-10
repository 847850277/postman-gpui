/* Session-only body drafts shared by the editor, cURL export and fixture preview. */
(function (root) {
  'use strict';
  const kinds = [['none','None'],['json','JSON'],['raw','Raw'],['urlencoded','URL encoded'],['multipart','Form-data'],['binary','Binary']];
  const rawTypes = {text:'text/plain',xml:'application/xml',html:'text/html',javascript:'application/javascript'};
  const row = () => ({enabled:true,key:'',value:'',type:'text',file:null});
  const copy = value => JSON.parse(JSON.stringify(value));
  function ensure(request) {
    if (request.bodyDrafts) return request.bodyDrafts;
    const hasBody = request.hasBody ?? (request.bodyExplicit || (request.body && ['POST','PUT','PATCH'].includes(request.method)));
    const format = request.bodyFormat || 'raw';
    request.bodyDrafts = {json:format === 'json' ? request.body || '' : '',raw:format === 'raw' ? request.body || '' : '',rawType:'text',urlencoded:[row()],multipart:[row()],binary:null};
    request.bodyFormat = hasBody ? format : 'none';
    sync(request);
    return request.bodyDrafts;
  }
  function rows(request) {
    return ensure(request)[request.bodyFormat].filter(r => r.enabled && (r.key || r.value || (r.type === 'file' && r.file)));
  }
  function sync(request) {
    const drafts = request.bodyDrafts;
    request.bodyExplicit = request.bodyFormat !== 'none';
    request.body = request.bodyFormat === 'urlencoded'
      ? new URLSearchParams(rows(request).map(r => [r.key,r.value])).toString()
      : ['json','raw'].includes(request.bodyFormat) ? drafts[request.bodyFormat] : '';
  }
  function select(request,kind) {
    if (!kinds.some(([value]) => value === kind)) throw new Error('Unknown body type');
    ensure(request); request.bodyFormat = kind; sync(request);
  }
  function contentType(request) {
    const drafts = ensure(request);
    return {none:'',json:'application/json',raw:rawTypes[drafts.rawType],urlencoded:'application/x-www-form-urlencoded',multipart:'multipart/form-data',binary:drafts.binary?.type || 'application/octet-stream'}[request.bodyFormat];
  }
  function headerInfo(request) {
    const explicit = request.headers.filter(h => h.enabled && h.key.toLowerCase() === 'content-type');
    const recommended = contentType(request);
    return {explicit,recommended,value:explicit.length ? explicit.map(h => h.value || (h.forceEmpty ? '(empty)' : '(suppressed)')).join(', ') : recommended};
  }
  function headers(request) {
    const info = headerInfo(request), result = copy(request.headers);
    // cURL supplies the multipart boundary itself. Do not freeze a boundary in the draft.
    if (!info.explicit.length && info.recommended && request.bodyFormat !== 'multipart') result.push({enabled:true,key:'Content-Type',value:info.recommended,note:'From body'});
    return result;
  }
  function validate(request) {
    const drafts = ensure(request), kind = request.bodyFormat;
    if (kind === 'json') {
      try { JSON.parse(drafts.json); } catch (_) { return {message:'Enter valid JSON, or choose None to send without a body.',field:'body-editor'}; }
    }
    if (kind === 'binary' && !drafts.binary) return {message:'Choose a file for the binary body.',field:'body-choose-file'};
    if (['urlencoded','multipart'].includes(kind)) {
      for (const [index,r] of drafts[kind].entries()) {
        if (!r.enabled || (!r.key && !r.value && !(r.type === 'file' && r.file))) continue;
        if (!r.key) return {message:'Add a key for field '+(index+1)+'.',field:'body-key-'+index};
        if (kind === 'multipart' && /[=\r\n]/.test(r.key)) return {message:'Form-data keys cannot contain “=” or line breaks in cURL export.',field:'body-key-'+index};
        if (kind === 'multipart' && r.type === 'file' && !r.file) return {message:'Choose a file for “'+r.key+'”.',field:'body-file-'+index};
      }
      if (kind === 'multipart' && !rows(request).length) return {message:'Add at least one form-data field, or choose None.',field:'body-add-field'};
    }
    return null;
  }
  function project(request) {
    ensure(request); sync(request);
    return {...copy(request),hasBody:request.bodyFormat !== 'none',headers:headers(request),
      multipart:request.bodyFormat === 'multipart' ? copy(rows(request)) : [],binary:request.bodyDrafts.binary};
  }
  function preview(request) {
    const drafts = ensure(request);
    sync(request);
    if (request.bodyFormat === 'json') return JSON.parse(drafts.json);
    if (request.bodyFormat === 'binary') return {file:copy(drafts.binary),preview:'File metadata only; no upload is performed.'};
    if (request.bodyFormat === 'multipart') return rows(request).map(r => r.type === 'file' ? {name:r.key,file:copy(r.file)} : {name:r.key,value:r.value});
    return request.bodyFormat === 'none' ? {} : request.body;
  }
  const file = value => ({name:value.name,size:value.size,type:value.type || '',lastModified:value.lastModified});
  const api = {kinds,rawTypes,row,ensure,sync,select,contentType,headerInfo,headers,validate,project,preview,file};
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.RequestBody = api;
})(globalThis);
