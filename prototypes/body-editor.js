(function (root) {
  'use strict';
  function attach({current,changed,openHeaders}) {
    const host = document.getElementById('pane-body');
    const $ = id => document.getElementById(id);
    const esc = value => String(value ?? '').replace(/[&<>"']/g,c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
    const icon = name => '<svg class="icon small" aria-hidden="true"><use href="#i-'+name+'"/></svg>';
    const size = bytes => bytes < 1024 ? bytes+' B' : bytes < 1048576 ? (bytes/1024).toFixed(1)+' KB' : (bytes/1048576).toFixed(1)+' MB';
    let showErrors = false;
    host.innerHTML = '<fieldset class="body-types"><legend>Body type</legend>'+RequestBody.kinds.map(([kind,label]) => '<label class="body-type"><input type="radio" name="body-type" value="'+kind+'">'+label+'</label>').join('')+'</fieldset><div class="body-meta"><span>Content-Type</span><code id="body-content-type"></code><button type="button" id="body-header-source"></button></div><div class="body-content" id="body-content"></div><p class="body-error" id="body-error" role="status" hidden></p>';
    function validate(focus = false) {
      const error = RequestBody.validate(current());
      host.querySelectorAll('[aria-invalid]').forEach(el => { el.removeAttribute('aria-invalid'); el.removeAttribute('aria-describedby'); });
      $('body-error').hidden = !error || !showErrors;
      $('body-error').textContent = error && showErrors ? error.message : '';
      if (error && showErrors) {
        const field = $(error.field);
        field?.setAttribute('aria-invalid','true'); field?.setAttribute('aria-describedby','body-error');
        if (focus) field?.focus();
      }
      return !error;
    }
    function refresh() {
      const request = current(), drafts = RequestBody.ensure(request), info = RequestBody.headerInfo(request);
      $('body-content-type').textContent = info.value || 'Not set';
      $('body-header-source').textContent = info.explicit.length ? 'Set in Headers' : info.recommended ? 'Automatic' : 'Add in Headers';
      const header = $('body-auto-header');
      header.hidden = !info.recommended || !!info.explicit.length;
      header.textContent = 'Content-Type: '+info.recommended+(request.bodyFormat === 'multipart' ? ' · Boundary generated when sending' : ' · From body');
      if ($('body-byte-count')) $('body-byte-count').textContent = size(new TextEncoder().encode(request.body).length);
      if ($('body-encoded')) $('body-encoded').textContent = request.body || 'No enabled fields';
      if ($('body-json-status')) {
        let valid = false; try { JSON.parse(drafts.json); valid = true; } catch (_) { /* Validation is shown on blur or submit. */ }
        $('body-json-status').textContent = valid ? 'Valid JSON' : 'JSON';
      }
      validate();
    }
    function update() { RequestBody.sync(current()); changed(); refresh(); }
    function renderRows() {
      const request = current(), kind = request.bodyFormat, multipart = kind === 'multipart';
      $('body-fields').innerHTML = request.bodyDrafts[kind].map((r,index) => {
        const labels = ' aria-label="', number = index+1;
        const key = '<label class="body-key"><span>Key</span><input id="body-key-'+index+'" type="text" data-body-field="key"'+labels+'Body key '+number+'" placeholder="Key" value="'+esc(r.key)+'" spellcheck="false"></label>';
        const type = multipart ? '<select class="body-row-type" data-body-field="type"'+labels+'Field type '+number+'"><option value="text"'+(r.type === 'text' ? ' selected' : '')+'>Text</option><option value="file"'+(r.type === 'file' ? ' selected' : '')+'>File</option></select>' : '';
        const value = multipart && r.type === 'file'
          ? '<div class="body-file-cell">'+(r.file ? '<span class="body-file-name" title="'+esc(r.file.name)+'">'+esc(r.file.name)+'</span>' : '')+'<button class="body-file-button" id="body-file-'+index+'" data-choose-file="'+index+'"'+labels+(r.file ? 'Change' : 'Choose')+' file for field '+number+'">'+(r.file ? 'Change…' : 'Choose file…')+'</button></div>'
          : '<label class="body-value"><span>Value</span><input type="text" data-body-field="value"'+labels+'Body value '+number+'" placeholder="Value" value="'+esc(r.value)+'" spellcheck="false"></label>';
        return '<div class="body-row'+(r.enabled ? '' : ' disabled')+'" data-body-row="'+index+'"><input type="checkbox" data-body-field="enabled"'+labels+'Enable body field '+number+'"'+(r.enabled ? ' checked' : '')+'>'+key+type+value+'<button class="icon-button body-remove" data-remove-field="'+index+'"'+labels+'Remove body field '+number+'">'+icon('trash')+'</button></div>';
      }).join('');
    }
    function renderContent() {
      const request = current(), drafts = RequestBody.ensure(request), kind = request.bodyFormat;
      if (kind === 'none') {
        $('body-content').innerHTML = '<div class="body-empty">'+icon('code')+'<strong>This request has no body</strong><span>Choose a body type to include data with this request.</span></div>';
      } else if (kind === 'json' || kind === 'raw') {
        $('body-content').innerHTML = '<div class="body-toolbar">'+(kind === 'json' ? '<label for="body-editor">JSON body</label><button class="text-button" id="format-body">Format JSON</button>' : '<label for="raw-language">Format</label><select id="raw-language" aria-label="Raw format"><option value="text">Text</option><option value="xml">XML</option><option value="html">HTML</option><option value="javascript">JavaScript</option></select>')+'</div><textarea class="body-editor" id="body-editor" aria-label="'+(kind === 'json' ? 'JSON body' : 'Raw body')+'" spellcheck="false" placeholder="'+(kind === 'json' ? '{ &quot;key&quot;: &quot;value&quot; }' : 'Enter a request body')+'"></textarea><div class="body-meta"><span id="'+(kind === 'json' ? 'body-json-status' : 'body-raw-status')+'">Plain text editor</span><span class="spacer"></span><span id="body-byte-count"></span></div>';
        $('body-editor').value = drafts[kind];
        if (kind === 'raw') $('raw-language').value = drafts.rawType;
      } else if (kind === 'urlencoded' || kind === 'multipart') {
        $('body-content').innerHTML = '<p class="body-caption">'+(kind === 'multipart' ? 'Combine text fields and files in one request.' : 'Keys and values are URL encoded automatically. Repeated keys are preserved.')+'</p><div class="body-grid '+kind+'"><div class="body-grid-head" aria-hidden="true"><span></span><span>Key</span>'+(kind === 'multipart' ? '<span>Type</span>' : '')+'<span>Value</span><span></span></div><div id="body-fields"></div></div><button class="add-row body-add" id="body-add-field">'+icon('plus')+'Add field</button>'+(kind === 'urlencoded' ? '<div class="body-meta"><span>Encoded body</span></div><pre class="body-encoded" id="body-encoded"></pre>' : '<p class="body-file-note">Files stay on your device. This preview uses file names and sizes only.</p>');
        renderRows();
      } else {
        const file = drafts.binary;
        $('body-content').innerHTML = '<div class="body-file-drop" id="body-file-drop">'+icon('import')+'<strong>'+(file ? esc(file.name) : 'Choose a file or drop it here')+'</strong><p class="body-caption">'+(file ? size(file.size)+' · '+esc(file.type || 'application/octet-stream') : 'Send a single file as the entire request body.')+'</p><div class="body-file-actions"><button class="body-file-button" id="body-choose-file" data-choose-file="binary">'+(file ? 'Change file…' : 'Choose file…')+'</button>'+(file ? '<button class="text-button" id="body-remove-file">Remove file</button>' : '')+'</div></div><p class="body-file-note">Files stay on your device. This preview uses file names and sizes only.</p>';
      }
      refresh();
    }
    function render() {
      RequestBody.ensure(current()); showErrors = false;
      host.querySelectorAll('[name="body-type"]').forEach(input => input.checked = input.value === current().bodyFormat);
      renderContent();
    }
    function chooseFile(target) {
      const request = current(), field = target === 'binary' ? null : request.bodyDrafts.multipart[Number(target)];
      const input = document.createElement('input'); input.type = 'file';
      input.addEventListener('change',() => {
        if (!input.files.length) return;
        if (field) field.file = RequestBody.file(input.files[0]); else request.bodyDrafts.binary = RequestBody.file(input.files[0]);
        RequestBody.sync(request);
        if (request === current()) { changed(); renderContent(); $(field ? 'body-file-'+target : 'body-choose-file')?.focus(); }
      },{once:true});
      input.click();
    }
    host.addEventListener('input',event => {
      const target = event.target, request = current();
      if (target.id === 'body-editor') { request.bodyDrafts[request.bodyFormat] = target.value; update(); }
      if (target.dataset.bodyField && target.type === 'text') {
        request.bodyDrafts[request.bodyFormat][Number(target.closest('[data-body-row]').dataset.bodyRow)][target.dataset.bodyField] = target.value; update();
      }
    });
    host.addEventListener('focusout',event => { if (event.target.id === 'body-editor' || event.target.dataset.bodyField) { showErrors = true; validate(); } });
    host.addEventListener('change',event => {
      const target = event.target, request = current();
      if (target.name === 'body-type') { RequestBody.select(request,target.value); showErrors = false; changed(); renderContent(); }
      if (target.id === 'raw-language') { request.bodyDrafts.rawType = target.value; update(); }
      if (['enabled','type'].includes(target.dataset.bodyField)) {
        const index = Number(target.closest('[data-body-row]').dataset.bodyRow);
        request.bodyDrafts[request.bodyFormat][index][target.dataset.bodyField] = target.type === 'checkbox' ? target.checked : target.value;
        update(); renderRows(); refresh();
        host.querySelector('[data-body-row="'+index+'"] [data-body-field="'+target.dataset.bodyField+'"]')?.focus();
      }
    });
    host.addEventListener('click',event => {
      const button = event.target.closest('button'); if (!button) return;
      const request = current(), drafts = request.bodyDrafts;
      if (button.id === 'body-header-source') { openHeaders(); return; }
      if (button.dataset.chooseFile !== undefined) { chooseFile(button.dataset.chooseFile); return; }
      if (button.id === 'format-body') {
        showErrors = true;
        if (validate(true)) { drafts.json = JSON.stringify(JSON.parse(drafts.json),null,2); $('body-editor').value = drafts.json; update(); }
      }
      if (button.id === 'body-add-field') {
        drafts[request.bodyFormat].push(RequestBody.row()); update(); renderRows(); refresh();
        $('body-key-'+(drafts[request.bodyFormat].length-1)).focus();
      }
      if (button.dataset.removeField !== undefined) {
        const index = Number(button.dataset.removeField);
        drafts[request.bodyFormat].splice(index,1); update(); renderRows(); refresh();
        ($('body-key-'+Math.min(index,drafts[request.bodyFormat].length-1)) || $('body-add-field')).focus();
      }
      if (button.id === 'body-remove-file') { drafts.binary = null; showErrors = false; update(); renderContent(); $('body-choose-file').focus(); }
    });
    for (const type of ['dragenter','dragover','dragleave','drop']) host.addEventListener(type,event => {
      const drop = event.target.closest('#body-file-drop'); if (!drop) return;
      event.preventDefault(); drop.classList.toggle('drag-over',type === 'dragenter' || type === 'dragover');
      if (type === 'drop' && event.dataTransfer.files.length) {
        if (event.dataTransfer.files.length > 1) { $('body-error').hidden = false; $('body-error').textContent = 'Choose one file for a binary body. Use Form-data for multiple files.'; return; }
        current().bodyDrafts.binary = RequestBody.file(event.dataTransfer.files[0]); update(); renderContent(); $('body-choose-file').focus();
      }
    });
    return {render,refresh,validate:() => { showErrors = true; return validate(true); }};
  }
  root.BodyEditor = {attach};
})(globalThis);
