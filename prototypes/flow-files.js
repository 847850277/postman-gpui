/* File-workflow design state. Browser snapshots are session-only; native GPUI owns real I/O. */
(function(root) {
 'use strict';
 const copy = value => JSON.parse(JSON.stringify(value));
 class Files {
  constructor() { this.entries = new Map(); }
  dirty(flow, pending = false) { return pending || flow.savedSource !== JSON.stringify(flow.document); }
  save(flow, path, {replace = false, exportCopy = false, pending = false, openFlows = []} = {}) {
   if (pending) throw Error('Fix pending document edits before saving or exporting.');
   if (!path.trim() || /[\\/]/.test(path)) throw Error('Enter a filename for this preview.');
   if (openFlows.some(other => other.filePath === path && (other !== flow || exportCopy))) throw Error('This file belongs to an open Flow. Choose another filename.');
   if (this.entries.has(path) && (flow.filePath !== path || exportCopy) && !replace) return false;
   this.entries.set(path, copy(flow.document));
   if (!exportCopy) { flow.filePath = path; flow.savedSource = JSON.stringify(flow.document); flow.dirty = false; }
   return true;
  }
  open(path) { if (!this.entries.has(path)) throw Error('This preview file is no longer available.'); return copy(this.entries.get(path)); }
 }
 function mount(api) {
  const files = api.files, $ = id => document.getElementById(id);
  let afterSave = null, operation = 'save', trigger = null;
  const dialog = document.createElement('dialog'); dialog.className = 'studio-dialog'; dialog.id = 'flow-file-dialog';
  dialog.innerHTML = '<form id="flow-file-form"><div class="dialog-head"><strong id="flow-file-dialog-title"></strong></div><div class="dialog-content"><label class="studio-field" id="flow-file-name-field"><span>File name</span><input class="studio-input" id="flow-file-name" required></label><div id="flow-file-entries"></div><p class="studio-hint">Design preview: files are snapshots in this browser session. The native app uses system file pickers and real .http.yml files.</p><p class="studio-hint">YAML includes entered credentials and defaults as plaintext. Runtime overrides are excluded.</p><p class="studio-error" id="flow-file-error" role="alert" hidden></p></div><div class="dialog-footer"><button type="button" class="studio-button" id="flow-file-cancel">Cancel</button><button type="button" class="studio-button" id="flow-file-discard" hidden>Discard changes</button><button class="studio-button primary" id="flow-file-submit">Save</button></div></form>';
  document.body.append(dialog);
  function finish() { dialog.close(); api.refresh(); }
  function show(kind, continuation = null) {
   operation = kind; afterSave = continuation; trigger = document.activeElement;
   $('flow-file-dialog-title').textContent = kind === 'stop' ? 'Stop running “'+api.current().document.flow.name+'” and close?' : kind === 'open' ? 'Recent flows' : kind === 'close' ? 'Save changes to “'+api.current().document.flow.name+'”?' : kind === 'export' ? 'Export copy' : 'Save Flow';
   $('flow-file-name-field').hidden = kind === 'open' || kind === 'close' || kind === 'stop';
   $('flow-file-entries').replaceChildren();
   $('flow-file-error').hidden = true;
   $('flow-file-discard').hidden = kind !== 'close';
   $('flow-file-submit').hidden = kind === 'open';
   $('flow-file-submit').textContent = kind === 'stop' ? 'Stop and close' : kind === 'export' ? 'Export copy' : 'Save';
   if (kind === 'stop') $('flow-file-entries').textContent='Closing stops the active preview and prevents later steps. Cancel keeps the Flow open and running. The native app also cancels real requests and polling waits.';
   $('flow-file-name').value = api.current().filePath || api.filename();
   if (kind === 'open') {
    if (!files.entries.size) $('flow-file-entries').textContent = 'Save a Flow in this preview to reopen it here.';
    for (const [path, doc] of files.entries) {
     const row = document.createElement('button'); row.type='button'; row.className='recent-item'; row.textContent=doc.flow.name+' · '+path;
     row.addEventListener('click', () => { api.open(path, files.open(path)); finish(); }); $('flow-file-entries').append(row);
    }
   }
   dialog.showModal(); (kind === 'open' || kind === 'close' || kind === 'stop' ? $('flow-file-cancel') : $('flow-file-name')).focus();
  }
  function closeAfterFiles() { if(api.current().run?.status==='running')show('stop');else api.close(); }
  function save(as = false, continuation = null) {
   if (!api.current().id) return;
   if (!as && api.current().filePath) {
    try { files.save(api.current(), api.current().filePath, {pending:api.pending(), openFlows:api.flows}); api.refresh(); continuation?.(); }
    catch (e) { show('save', continuation); $('flow-file-error').textContent=e.message; $('flow-file-error').hidden=false; }
   } else show('save', continuation);
  }
  $('flow-file-form').addEventListener('submit', async event => {
   event.preventDefault();
   if (operation === 'stop') { $('flow-file-submit').disabled=true; $('flow-file-cancel').disabled=true; await api.stop(); $('flow-file-submit').disabled=false; $('flow-file-cancel').disabled=false; dialog.close(); api.close(); return; }
   if (operation === 'close') { dialog.close(); setTimeout(()=>save(false, closeAfterFiles),0); return; }
   try {
    const path = $('flow-file-name').value.trim();
    const replacing = $('flow-file-submit').textContent === 'Replace file';
    if (!files.save(api.current(), path, {pending:api.pending(), openFlows:api.flows, replace:replacing, exportCopy:operation==='export'})) {
     $('flow-file-error').textContent='This destination already exists. Replace it or choose another filename.'; $('flow-file-error').hidden=false; $('flow-file-submit').textContent='Replace file'; return;
    }
    if (operation === 'export') api.download(path);
    const continuation = afterSave; finish(); continuation?.();
   } catch (e) { $('flow-file-error').textContent=e.message; $('flow-file-error').hidden=false; }
  });
  $('flow-file-name').addEventListener('input', () => { $('flow-file-submit').textContent=operation==='export'?'Export copy':'Save'; });
  $('flow-file-cancel').addEventListener('click', () => dialog.close());
  $('flow-file-discard').addEventListener('click', () => { dialog.close(); closeAfterFiles(); });
  dialog.addEventListener('cancel', event => { if($('flow-file-submit').disabled)event.preventDefault(); });
  dialog.addEventListener('close', () => { afterSave = null; trigger?.focus(); });
  $('save-flow').addEventListener('click', () => save());
  $('open-flow').addEventListener('click', () => show('open'));
  $('flow-recent-files').addEventListener('click', () => show('open'));
  $('flow-file-actions').addEventListener('change', event => {
   const value=event.target.value; event.target.value='';
   if(value==='save-as')save(true); if(value==='recent')show('open');
   if(value==='close'&&api.current().id) { if(files.dirty(api.current(),api.pending()))show('close'); else closeAfterFiles(); }
  });
  return {save, export:()=>{if(api.current().id)show('export');}, open:()=>show('open'), close:()=>{if(!api.current().id)return;if(files.dirty(api.current(),api.pending()))show('close');else closeAfterFiles();}};
 }
 const api = {Files, mount}; if (typeof module === 'object') module.exports=api; else root.FlowFilePreview=api;
})(typeof globalThis === 'object' ? globalThis : this);
