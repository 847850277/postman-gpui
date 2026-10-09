/* Session-only request navigation; no request data is stored by this component. */
(() => {
 'use strict';
 const $ = id => document.getElementById(id);
 const esc = value => String(value).replace(/[&<>"']/g, ch => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));
 const method = request => '<span class="method ' + esc(request.method.toLowerCase()) + '">' + esc(request.method) + '</span>';
 function attach(config) {
  const strip = $('request-tabs'), bar = $('request-tabs-bar'), trigger = $('open-request-tabs');
  const dialog = $('request-list-dialog'), filter = $('request-list-filter'), results = $('request-list-results');
  let frame = 0, revealPending = false, focusSelectedOnClose = false;
  const selected = () => strip.querySelector('[aria-selected="true"]');
  function revealSelected() {
   const active = selected(); if (!active || !strip.clientWidth) return;
   const bounds = strip.getBoundingClientRect(), tab = active.getBoundingClientRect();
   if (tab.top < bounds.top) strip.scrollTop += tab.top - bounds.top;
   else if (tab.bottom > bounds.bottom) strip.scrollTop += tab.bottom - bounds.bottom;
  }
  function positionDialog() {
   if (!dialog.open) return;
   const bounds = trigger.getBoundingClientRect();
   const top = Math.min(bounds.bottom + 8, Math.max(12, innerHeight - 180));
   dialog.style.top = top + 'px';
   dialog.style.left = Math.max(12, Math.min(bounds.right - dialog.offsetWidth, innerWidth - dialog.offsetWidth - 12)) + 'px';
   dialog.style.maxHeight = Math.min(540, innerHeight - top - 12) + 'px';
  }
  function refresh() {
   frame = 0;
   if (!bar.clientWidth) return;
   const rowHeight = parseFloat(getComputedStyle(bar).getPropertyValue('--request-row-height'));
   bar.style.setProperty('--visible-tab-rows', Math.max(2, Math.min(8, Math.floor(innerHeight * .32 / rowHeight))));
   if (revealPending) { revealSelected(); revealPending = false; }
   positionDialog();
  }
  function schedule(reveal = false) {
   revealPending ||= reveal;
   if (!frame) frame = requestAnimationFrame(refresh);
  }
  function renderList() {
   const query = filter.value.trim().toLowerCase(), requests = config.list();
   const matches = requests.filter(request => (request.title + ' ' + request.method + ' ' + request.url).toLowerCase().includes(query));
   $('request-list-count').textContent = requests.length;
   $('request-list-summary').textContent = query ? matches.length + ' of ' + requests.length + ' requests' : requests.length + ' open requests';
   results.innerHTML = matches.length ? '<ul>' + matches.map(request => {
    const active = request.id === config.active();
    return '<li><button class="request-list-item" data-select-request="' + esc(request.id) + '"' + (active ? ' aria-current="true"' : '') +
     ' title="' + esc(request.title + '\n' + (request.url || 'No URL yet')) + '">' + method(request) + '<span class="request-list-details"><span class="request-list-name"><span>' + esc(request.title) +
     '</span>' + (request.dirty ? '<span class="request-list-dirty" aria-label="Unsaved changes"></span>' : '') + '</span><span class="request-list-url">' + esc(request.url || 'No URL yet') +
     '</span></span>' + (active ? '<svg class="icon" aria-label="Current request"><use href="#i-check"/></svg>' : '') + '</button></li>';
   }).join('') + '</ul>' : '<p class="request-list-empty">No matching requests.</p>';
   results.scrollTop = 0;
  }
  function render() {
   const restoreFocus = strip.contains(document.activeElement), scroll = strip.scrollTop, requests = config.list();
   strip.innerHTML = requests.map(request => '<button class="request-tab" role="tab" aria-selected="' + (request.id === config.active()) + '" tabindex="' + (request.id === config.active() ? '0' : '-1') +
    '" data-request="' + esc(request.id) + '" title="' + esc(request.method + ' ' + request.title + '\n' + (request.url || 'No URL yet')) + '">' + method(request) +
    '<span class="name">' + esc(request.title) + '</span>' + (request.dirty ? '<span class="tab-dot" aria-label="Unsaved changes"></span>' : '') + '</button>').join('');
   strip.scrollTop = scroll;
   $('open-request-count').textContent = requests.length;
   trigger.title = 'Show all ' + requests.length + ' open requests';
   if (restoreFocus) selected()?.focus({preventScroll:true});
   if (dialog.open) renderList();
   schedule(true);
  }
  strip.addEventListener('click', event => {
   const tab = event.target.closest('[data-request]');
   if (tab) config.activate(tab.dataset.request);
  });
  strip.addEventListener('keydown', event => {
   if (!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home','End'].includes(event.key)) return;
   const tabs = [...strip.children], index = tabs.indexOf(document.activeElement);
   if (index < 0) return;
   event.preventDefault();
   let target = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
   if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
    const origin = tabs[index].getBoundingClientRect(), direction = event.key === 'ArrowDown' ? 1 : -1;
    const candidates = tabs.map((tab, i) => ({index:i, rect:tab.getBoundingClientRect()}))
     .filter(item => (item.rect.top - origin.top) * direction > 1)
     .sort((a, b) => Math.abs(a.rect.top - origin.top) - Math.abs(b.rect.top - origin.top) || Math.abs(a.rect.left - origin.left) - Math.abs(b.rect.left - origin.left));
    target = candidates[0]?.index ?? index;
   }
   config.activate(tabs[target].dataset.request); selected()?.focus({preventScroll:true});
  });
  $('new-tab').addEventListener('click', config.newRequest);
  trigger.addEventListener('click', () => {
   filter.value = ''; focusSelectedOnClose = false; renderList(); dialog.showModal(); positionDialog();
   trigger.setAttribute('aria-expanded','true'); filter.focus({preventScroll:true});
   const active = results.querySelector('[aria-current="true"]');
   if (active) results.scrollTop = active.offsetTop - results.offsetTop - Math.max(0, (results.clientHeight - active.offsetHeight) / 2);
  });
  $('close-request-list').addEventListener('click', () => dialog.close());
  dialog.addEventListener('close', () => {
   trigger.setAttribute('aria-expanded','false');
   (focusSelectedOnClose ? selected() : trigger)?.focus({preventScroll:true});
   focusSelectedOnClose = false;
  });
  filter.addEventListener('input', renderList);
  results.addEventListener('click', event => {
   const item = event.target.closest('[data-select-request]'); if (!item) return;
   focusSelectedOnClose = true; dialog.close(); config.activate(item.dataset.selectRequest);
  });
  dialog.addEventListener('keydown', event => {
   event.stopPropagation();
   if (event.metaKey || event.ctrlKey) {
    if (['s','k','Enter'].includes(event.key)) event.preventDefault();
    return;
   }
   const items = [...results.querySelectorAll('button')], index = items.indexOf(document.activeElement);
   if (event.target === filter && event.key === 'Enter') { event.preventDefault(); items[0]?.click(); return; }
   if (!['ArrowDown','ArrowUp','Home','End'].includes(event.key) || (!items.length) || (index < 0 && event.target !== filter)) return;
   if (event.target === filter && ['Home','End'].includes(event.key)) return;
   event.preventDefault();
   const target = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : index < 0 ? (event.key === 'ArrowDown' ? 0 : items.length - 1) : (index + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
   items[target].focus();
  });
  new ResizeObserver(() => schedule(true)).observe(bar);
  window.addEventListener('resize', () => schedule(true));
  document.fonts.ready.then(() => schedule(true));
  return {render};
 }
 window.RequestTabs = {attach};
})();
