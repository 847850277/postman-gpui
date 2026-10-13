/* Local fixtures for the shipped HTTPBingo examples. Never sends a request. */
(function(root) {
 'use strict';
 const baseUrl = 'https://httpbingo.org';
 const sampleBody = {name:'Maya Chen'};
 const sampleUuid = 'a68bc18f-89c5-4e45-b358-547820302c93';
 const statusText = code => ({200:'OK',201:'Created',204:'No Content',400:'Bad Request',404:'Not Found',405:'Method Not Allowed',500:'Internal Server Error'}[code] || '');
 function values(entries) {
  const result = Object.create(null);
  for (const [key,value] of entries) (result[key] ||= []).push(value);
  return {...result};
 }
 function response({url,route,method='GET',body,bodyFormat='none',headers=[]}) {
  const address = new URL(url), path = new URL(route ?? url,baseUrl).pathname;
  const finish = (code,data) => ({code,status:(code+' '+statusText(code)).trim(),data:method === 'HEAD' ? null : data});
  const status = /^\/status\/(\d{3})$/.exec(path);
  if (status) return finish(Number(status[1]),null);
  if (path === '/uuid') return finish(200,{uuid:sampleUuid});
  const required = {'/get':'GET','/post':'POST','/put':'PUT','/patch':'PATCH','/delete':'DELETE','/head':'HEAD'}[path];
  if (required && method !== required && !(method === 'HEAD' && required === 'GET')) return finish(405,null);
  const echoedHeaders = values(headers.filter(h => h.enabled !== false && h.key).map(h => [h.key,h.value]));
  if (path === '/headers') return finish(200,{headers:echoedHeaders});
  if (!required && path !== '/anything' && !path.startsWith('/anything/')) return finish(404,null);
  const data = {args:values(address.searchParams),headers:echoedHeaders,method,origin:'127.0.0.1',url:address.href};
  if (path !== '/get' && path !== '/head') {
   Object.assign(data,{data:'',files:{},form:{},json:null});
   if (bodyFormat === 'json') {data.json=body;data.data=JSON.stringify(body);}
   else if (bodyFormat === 'urlencoded') data.form=values(new URLSearchParams(body));
   else if (bodyFormat === 'multipart') {
    data.form=values(body.filter(row => !row.file).map(row => [row.name,row.value]));
    data.files=values(body.filter(row => row.file).map(row => [row.name,'[Local preview: '+row.file.name+'; file contents are not read]']));
   } else if (bodyFormat === 'binary') data.data='[Local preview: '+body.file.name+'; file contents are not read]';
   else if (bodyFormat === 'raw') data.data=body;
  }
  return finish(200,data);
 }
 const api={baseUrl,sampleBody,sampleUuid,response};
 if(typeof module !== 'undefined' && module.exports) module.exports=api; else root.HTTPBingoDemo=api;
})(globalThis);
