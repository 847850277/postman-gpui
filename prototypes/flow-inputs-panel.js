/* Focused inspector extension: the rest of the Flow screen remains the design baseline. */
window.FlowInputsPanel=class {
 constructor(host,flow,base,edited,back){this.host=host;this.flow=flow;this.state=flow.inputDrafts??=new FlowInputDrafts.Inputs(flow.document.flow.inputs);this.base=base;this.edited=edited;this.back=back;this.render();}
 render(){
  const E=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])),state=this.state,def=state.definitions;
  const button=(action,text,id,extra='')=>`<button class="studio-button quiet" type="button" data-input-action="${action}" ${id==null?'':`data-row="${id}"`} ${extra}>${text}</button>`;
  const editor=(r,key)=>{
   const v=r[key],sensitive=r.sensitive||r.applied?.sensitive,hidden=sensitive&&!r.revealed;let textAllowed=false;try{textAllowed=typeof FlowInputDrafts.parse(v)==='string';}catch{}
   return `<div class="input-value-modes">${button(key+'-text','Text',r.id,`title="Text mode requires a JSON string; other JSON types keep JSON mode" aria-pressed="${v.mode==='text'}" ${hidden||!textAllowed?'disabled':''}`)}${button(key+'-json','JSON',r.id,`aria-pressed="${v.mode==='json'}" ${hidden?'disabled':''}`)}<span class="spacer"></span>${sensitive?button('reveal',hidden?'Reveal / edit':'Hide',r.id):''}</div>${hidden?'<p class="studio-hint">Sensitive value hidden</p>':`<textarea class="studio-input input-value-source" aria-label="${key==='default'?'Default':'Run'} value" data-input-field="${key}" data-row="${r.id}" spellcheck="false">${E(v.source)}</textarea>`}`;
  };
  this.host.innerHTML=`<div class="inspector-heading"><strong>Flow inputs</strong><span class="spacer"></span>${button('back','Step details')}</div><div class="inspector-tabs input-view-tabs">${button('run','Run inputs',null,`aria-pressed="${!def}"`)}${button('definitions','Definitions',null,`aria-pressed="${def}"`)}</div><div class="inspector-form input-fields"><p class="studio-hint">${def?'Declare names, defaults, and sensitivity. No default means a required input.':'Prepare values for a future run. Overrides stay separate from document defaults. Environment supplies base_url when declared.'}</p>${!state.rows.length?'<p class="studio-hint">No inputs declared. Add an input to make this flow configurable.</p>':''}${state.rows.map(r=>{
   const inherited=state.inherited(r,this.base()),hidden=r.sensitive||r.applied?.sensitive;
   return `<section class="input-definition-row" data-input-row="${r.id}">${def?`<div class="input-row-title"><input class="studio-input" aria-label="Input name" placeholder="Input name" data-input-field="name" data-row="${r.id}" value="${E(r.name)}">${button('remove','Remove',r.id)}</div><div class="input-options"><label><input type="checkbox" data-input-field="hasDefault" data-row="${r.id}" ${r.hasDefault?'checked':''}> Default value</label><label><input type="checkbox" data-input-field="sensitive" data-row="${r.id}" ${r.sensitive?'checked':''}> Sensitive</label></div>${r.hasDefault?editor(r,'default'):'<p class="studio-hint">Required unless a run value is supplied.</p>'}`:`<div class="input-row-title"><strong>${E(r.applied?.name||'Unnamed input')}</strong><span class="spacer"></span><span class="studio-hint">${r.overridden?'Override':inherited.source}</span></div>${r.overridden?editor(r,'runtime'):`<pre class="input-inherited">${hidden?'Sensitive value hidden':Object.hasOwn(inherited,'value')?E(typeof inherited.value==='string'?inherited.value:JSON.stringify(inherited.value)):'No value supplied'}</pre>`}<div class="input-options"><label><input type="checkbox" data-input-field="overridden" data-row="${r.id}" ${r.overridden?'checked':''} ${r.applied?'':'disabled'}> Override for this session</label>${r.overridden?button('reset','Reset',r.id):''}</div>`}<p class="studio-error" data-input-error="${r.id}" role="status"></p></section>`;
  }).join('')}${button('add','＋ Add input')}<p class="studio-hint">${def?'Renaming or removing inputs leaves existing references unchanged. Sensitive defaults remain in the document; the flag does not encrypt them.':'Session only. This HTML preview uses local fixtures; native execution is a later stage.'}</p></div>`;
  this.host.oninput=e=>{const field=e.target.dataset.inputField;if(!field||e.target.type==='checkbox')return;const row=state.rows.find(r=>r.id===Number(e.target.dataset.row));if(field==='name')row.name=e.target.value;else row[field].source=e.target.value;if(field!=='runtime')this.commit();this.errors();};
  this.host.onchange=e=>{if(e.target.type!=='checkbox')return;const row=state.rows.find(r=>r.id===Number(e.target.dataset.row)),field=e.target.dataset.inputField;if(!row)return;if(field==='overridden')state.override(row,e.target.checked,this.base());else{row[field]=e.target.checked;row.revealed=false;this.commit();}this.render();};
  this.host.onclick=e=>{const target=e.target.closest('[data-input-action]');if(!target)return;const action=target.dataset.inputAction,row=state.rows.find(r=>r.id===Number(target.dataset.row));
   if(action==='back'){state.conceal();this.host.oninput=this.host.onchange=this.host.onclick=null;this.back();return;}
   if(action==='add'){state.conceal();const added=state.add();this.commit();this.render();const name=this.host.querySelector(`[data-input-field="name"][data-row="${added.id}"]`);name?.focus();name?.scrollIntoView({block:'nearest'});return;}
   if(action==='run'||action==='definitions'){state.conceal();state.definitions=action==='definitions';}
   else if(action==='remove'){const index=state.rows.indexOf(row);state.remove(row.id);this.commit();this.render();const next=state.rows[index]||state.rows.at(-1);const focus=next?this.host.querySelector(`[data-input-field="name"][data-row="${next.id}"]`):this.host.querySelector('[data-input-action="add"]');focus?.focus();focus?.scrollIntoView({block:'nearest'});return;}
   else if(action==='reset')state.override(row,false,this.base());
   else if(action==='reveal')row.revealed=!row.revealed;
   else {const [key,mode]=action.split('-');try{state.switchMode(row[key],mode);}catch{}if(key==='default')this.commit();}
   this.render();
  };
  this.errors();
 }
 commit(){this.flow.document.flow.inputs=this.state.apply();this.edited();}
 errors(){
  for(const row of this.state.rows){
   let message=row.error||'';
   const key=this.state.definitions?'default':'runtime',active=this.state.definitions?row.hasDefault:row.overridden;
   if(active){
    let parsed,valid=true;
    try{parsed=FlowInputDrafts.parse(row[key]);}catch(e){message=e.message;valid=false;}
    const hidden=(row.sensitive||row.applied?.sensitive)&&!row.revealed;
    for(const mode of ['text','json']){
     const control=this.host.querySelector(`[data-input-action="${key}-${mode}"][data-row="${row.id}"]`);
     if(control)control.disabled=hidden||!valid||(mode==='text'&&typeof parsed!=='string');
    }
   }
   this.host.querySelector(`[data-input-error="${row.id}"]`).textContent=message;
  }
 }
};
