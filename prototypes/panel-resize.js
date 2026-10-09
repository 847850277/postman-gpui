/* Persistent layout preferences only; request and flow data remain session-local. */
(() => {
 'use strict';
 const storageKey = 'postman-gpui:prototype:panel-layout:v1';
 let preferences = {};
 try {
  const saved = JSON.parse(localStorage.getItem(storageKey));
  if (saved && typeof saved === 'object' && !Array.isArray(saved)) preferences = saved;
 } catch (_) { /* Resizing still works when storage is unavailable. */ }
 const save = () => { try { localStorage.setItem(storageKey, JSON.stringify(preferences)); } catch (_) {} };
 const resolve = value => typeof value === 'function' ? value() : value;
 const clamp = (value, min, max) => Math.max(min, Math.min(max, value));
 const help = document.createElement('span');
 help.id = 'resize-help'; help.className = 'resize-help';
 help.textContent = 'Drag to resize. Arrow keys adjust; Shift adjusts faster. Double-click to reset. Enter opens size controls. Escape cancels a drag.';
 document.body.append(help);
 const popover = document.createElement('div');
 popover.className = 'resize-popover'; popover.hidden = true;
 popover.setAttribute('role', 'dialog'); popover.setAttribute('aria-labelledby', 'resize-title');
 popover.innerHTML = '<div class="resize-popover-title"><strong id="resize-title"></strong><button class="icon-button" data-resize="close" aria-label="Close panel size">×</button></div>' +
  '<div class="resize-value-row"><button data-resize="less" aria-label="Decrease panel size">−</button><label class="resize-value-field"><input id="resize-value" type="number" aria-label="Panel size"><span id="resize-unit"></span></label><button data-resize="more" aria-label="Increase panel size">+</button></div>' +
  '<p id="resize-limits"></p><div class="resize-popover-footer"><button data-resize="reset">Reset to default</button><button class="resize-done" data-resize="close">Done</button></div>';
 document.body.append(popover);
 const valueInput = popover.querySelector('input');
 let popupOwner = null;
 function closePopup(focus = false) {
  const owner = popupOwner;
  popupOwner = null; popover.hidden = true;
  if (focus) owner?.handle.focus({preventScroll:true});
 }
 function syncPopup(owner) {
  if (popupOwner !== owner) return;
  const m = owner.measure(); if (!m) return closePopup();
  const value = owner.displayValue(m);
  valueInput.min = m.minValue; valueInput.max = m.maxValue; valueInput.step = m.step;
  valueInput.value = Math.round(value);
  popover.querySelector('#resize-title').textContent = resolve(owner.config.label);
  popover.querySelector('#resize-unit').textContent = m.unit;
  popover.querySelector('#resize-limits').textContent = `${m.minValue}–${m.maxValue}${m.unit}. Saved for this browser.`;
  popover.querySelector('[data-resize="less"]').disabled = value <= (m.ratio?m.min/m.total*100:m.min) + 0.000001;
  popover.querySelector('[data-resize="more"]').disabled = value >= (m.ratio?m.max/m.total*100:m.max) - 0.000001;
 }
 function openPopup(owner, point) {
  popupOwner = owner; popover.hidden = false; syncPopup(owner);
  if (!popupOwner) return;
  const rect = owner.handle.getBoundingClientRect();
  const x = point?.x ?? rect.left + rect.width / 2;
  const y = point?.y ?? rect.top + rect.height / 2;
  popover.style.left = clamp(x + 16, 12, window.innerWidth - popover.offsetWidth - 12) + 'px';
  popover.style.top = clamp(y + 16, 12, window.innerHeight - popover.offsetHeight - 12) + 'px';
  valueInput.focus({preventScroll:true}); valueInput.select();
 }
 popover.addEventListener('click', event => {
  const action = event.target.closest('[data-resize]')?.dataset.resize;
  if (!action || !popupOwner) return;
  const owner = popupOwner, m = owner.measure(); if (!m) return closePopup();
  if (action === 'close') return closePopup(true);
  if (action === 'reset') owner.reset();
  else owner.setDisplay(owner.displayValue(m) + (action === 'more' ? m.step : -m.step));
  syncPopup(owner);
 });
 valueInput.addEventListener('change', () => {
  if (!popupOwner) return;
  if (Number.isFinite(valueInput.valueAsNumber)) popupOwner.setDisplay(valueInput.valueAsNumber);
  syncPopup(popupOwner);
 });
 popover.addEventListener('keydown', event => {
  event.stopPropagation();
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') event.preventDefault();
  if (event.key === 'Escape' || (event.key === 'Enter' && event.target === valueInput)) {
   event.preventDefault();
   if (event.key === 'Enter' && Number.isFinite(valueInput.valueAsNumber)) popupOwner?.setDisplay(valueInput.valueAsNumber);
   closePopup(true);
  }
  if (event.key === 'Tab') {
   const controls = [...popover.querySelectorAll('button,input')].filter(el => !el.disabled);
   const first = controls[0], last = controls.at(-1);
   if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
   else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  }
 });
 document.addEventListener('pointerdown', event => {
  if (popupOwner && !popover.contains(event.target) && !popupOwner.handle.contains(event.target)) closePopup();
 });
 window.addEventListener('resize', () => closePopup());

 function attach(config) {
  const {container, handle} = config;
  let current = 0, drag = null, suppressClick = false, frame = 0;
  handle.setAttribute('role', 'separator'); handle.tabIndex = 0;
  handle.setAttribute('aria-describedby', help.id);
  handle.title = 'Drag to resize · Click for size controls · Double-click to reset';
  function measure() {
   if (!handle.getClientRects().length || !container.clientWidth || !container.clientHeight) return null;
   const axis = resolve(config.axis), ratio = !!config.ratio;
   const total = (axis === 'x' ? container.clientWidth - handle.offsetWidth : container.clientHeight - handle.offsetHeight);
   if (total <= 0) return null;
   const limits = config.limits(total), min = Math.max(0, limits[0]), max = Math.max(min, Math.min(total, limits[1]));
   return {axis, ratio, total, min, max, key:resolve(config.key), unit:ratio?'%':'px', step:ratio?2:16,
    minValue:Math.round(ratio?min/total*100:min), maxValue:Math.round(ratio?max/total*100:max)};
  }
  function preferred(m) {
   const value = preferences[m.key];
   return typeof value === 'number' && Number.isFinite(value) && value > 0 && (!m.ratio || value < 1) ? value : resolve(config.initial);
  }
  function refresh() {
   const m = measure();
   if (!m) { if (drag) finish(true); if (popupOwner === owner) closePopup(); return; }
   if (drag && (drag.key !== m.key || drag.axis !== m.axis)) finish(true);
   const wanted = preferred(m);
   current = clamp(m.ratio ? wanted*m.total : wanted, m.min, m.max);
   config.apply(current, m);
   handle.setAttribute('aria-label', resolve(config.label));
   handle.setAttribute('aria-orientation', m.axis === 'x' ? 'vertical' : 'horizontal');
   handle.setAttribute('aria-valuemin', m.minValue); handle.setAttribute('aria-valuemax', m.maxValue);
   const display = displayValue(m);
   handle.setAttribute('aria-valuenow', Math.round(display));
   handle.setAttribute('aria-valuetext', Math.round(display) + m.unit);
   syncPopup(owner);
  }
  function displayValue(m) { return m.ratio ? current/m.total*100 : current; }
  function setPixels(value, persist = true) {
   const m = measure(); if (!m) return;
   value = clamp(value, m.min, m.max);
   preferences[m.key] = m.ratio ? value/m.total : value;
   refresh(); if (persist) save();
  }
  function setDisplay(value) {
   const m = measure(); if (!m) return;
   setPixels(m.ratio ? value/100*m.total : value);
  }
  function reset() { const m = measure(); if (!m) return; delete preferences[m.key]; refresh(); save(); }
  function finish(cancel = false) {
   if (!drag) return;
   const ended = drag; drag = null;
   if (cancel) {
    if (ended.saved === undefined) delete preferences[ended.key]; else preferences[ended.key] = ended.saved;
   } else if (ended.moved) save();
   suppressClick = ended.moved || cancel;
   handle.classList.remove('is-dragging'); delete document.body.dataset.resizing;
   if (handle.hasPointerCapture(ended.id)) handle.releasePointerCapture(ended.id);
   refresh();
  }
  const owner = {config, handle, measure, displayValue, refresh, reset, setDisplay};
  handle.addEventListener('pointerdown', event => {
   if (event.button !== 0 || !event.isPrimary) return;
   const m = measure(); if (!m) return;
   event.preventDefault(); closePopup(); suppressClick = false;
   handle.focus({preventScroll:true});
   drag = {id:event.pointerId, axis:m.axis, key:m.key, saved:preferences[m.key], start:m.axis==='x'?event.clientX:event.clientY, value:current, moved:false};
   handle.setPointerCapture(event.pointerId);
  });
  handle.addEventListener('pointermove', event => {
   if (!drag || drag.id !== event.pointerId) return;
   const delta = (drag.axis === 'x' ? event.clientX : event.clientY) - drag.start;
   if (!drag.moved && Math.abs(delta) < 3) return;
   drag.moved = true; handle.classList.add('is-dragging'); document.body.dataset.resizing = drag.axis;
   setPixels(drag.value + delta*(config.fromEnd?-1:1), false);
  });
  handle.addEventListener('pointerup', event => { if (event.pointerId === drag?.id) finish(); });
  handle.addEventListener('pointercancel', event => { if (event.pointerId === drag?.id) finish(true); });
  handle.addEventListener('lostpointercapture', event => { if (event.pointerId === drag?.id) finish(true); });
  window.addEventListener('blur', () => finish(true));
  handle.addEventListener('click', event => {
   if (suppressClick) { suppressClick = false; return; }
   openPopup(owner, event.detail ? {x:event.clientX,y:event.clientY} : null);
  });
  handle.addEventListener('dblclick', () => { closePopup(); reset(); handle.focus({preventScroll:true}); });
  handle.addEventListener('keydown', event => {
   const m = measure(); if (!m) return;
   if (event.key === 'Escape') { finish(true); return; }
   if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); openPopup(owner); return; }
   const negative = m.axis === 'x' ? 'ArrowLeft' : 'ArrowUp', positive = m.axis === 'x' ? 'ArrowRight' : 'ArrowDown';
   if (![negative,positive,'Home','End'].includes(event.key)) return;
   event.preventDefault();
   if (event.key === 'Home') return setPixels(m.min);
   if (event.key === 'End') return setPixels(m.max);
   const delta = (event.key===negative?-1:1)*(config.fromEnd?-1:1)*(event.shiftKey?5:1)*m.step;
   setDisplay(displayValue(m) + delta);
  });
  const observer = new ResizeObserver(() => { cancelAnimationFrame(frame); frame = requestAnimationFrame(refresh); });
  observer.observe(container);
  refresh();
  return owner;
 }
 window.PanelResize = {attach};
})();
