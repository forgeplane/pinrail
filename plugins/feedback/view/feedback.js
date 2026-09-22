(function () {
  'use strict';
  const C = FeedbackCore, esc = Wicket.escape, md = Wicket.markdown, ico = name => Wicket.icon(name, {size: 14});
  const app = document.getElementById('app');
  let payload, state, previous, showErrors = false, shellErrors = [], opened = new Set(), composing = false;
  // the rail of groups: shown by default, and the choice is the shell's to
  // keep, so it holds for the next set of questions too
  let railOpen = true;
  const plugin = Wicket.connect({
    resize: 'fill',
    onInit({gate, draft, previous: old, settings}) {
      applySettings(settings);
      showErrors = false; shellErrors = []; previous = old; opened = new Set();
      try { payload = C.validate(gate.payload); state = C.restore(payload, gate.decision?.data || draft); }
      catch (e) { payload = null; app.innerHTML = `<div class="fatal" role="alert"><h1>Unable to show these questions</h1><p>${esc(e.message)}</p></div>`; plugin.status({label:'Questions need correction'}); return; }
      render();
    },
    onCollect() {
      if (!payload || plugin.readonly) return;
      try { plugin.submit(C.decision(payload, state)); }
      catch { showErrors = true; render(); const first = app.querySelector('[aria-invalid="true"]'); first?.focus(); first?.scrollIntoView({block:'center'}); }
    },
    onViolations(errors) { shellErrors = errors; render(); app.querySelector('[role="alert"]')?.focus(); },
    onSubmitted() { if (!payload) return; state = C.restore(payload, plugin.gate.decision?.data); shellErrors = []; showErrors = false; render(); },
    onSettings(settings) { applySettings(settings); render(); },
  });
  function applySettings(settings) {
    if (typeof settings?.rail_open === 'boolean') railOpen = settings.rail_open;
  }
  const instructions = { single_choice:'Choose one', multiple_choice:'Select all that apply', text:'Free text', boolean:'Choose yes or no', checkbox:'Acknowledgment' };
  function controls(q, error) {
    const value = state.values[q.id], disabled = plugin.readonly ? 'disabled' : '';
    const invalid = error ? 'aria-invalid="true"' : '';
    const describedBy = `hint-${q.id}${q.description ? ' desc-' + q.id : ''}${error ? ' error-' + q.id : ''}`;
    if (q.type === 'text') return `<textarea id="answer-${q.id}" class="field text-answer" data-answer="${q.id}" rows="3" ${disabled} ${invalid} aria-labelledby="prompt-${q.id}" aria-describedby="${describedBy}" placeholder="${esc(q.placeholder || 'Your response…')}">${esc(value || '')}</textarea><div class="text-limit">${q.max_length ? `Up to ${q.max_length} characters` : ''}</div>`;
    if (q.type === 'checkbox') return `<label class="acknowledgment ${value === true ? 'is-selected' : ''}"><input id="answer-${q.id}" type="checkbox" data-answer="${q.id}" ${value === true ? 'checked' : ''} ${disabled} ${invalid} aria-labelledby="prompt-${q.id}" aria-describedby="${describedBy}"><span>${esc(q.checkbox_label || 'I confirm')}</span></label>`;
    const options = q.type === 'boolean' ? [{id:'true',label:'Yes'}, {id:'false',label:'No'}] : q.options;
    const multiple = q.type === 'multiple_choice';
    return `<div class="choices ${q.type === 'boolean' ? 'boolean-choices' : ''}">${options.map((o, index) => {
      const optionValue = q.type === 'boolean' ? o.id === 'true' : o.id;
      const selected = multiple ? (value || []).includes(o.id) : value === optionValue;
      const rec = q.recommendation && (multiple ? q.recommendation.answer.includes(o.id) : q.recommendation.answer === optionValue);
      return `<label class="choice ${selected ? 'is-selected' : ''}"><input id="answer-${q.id}-${index}" type="${multiple ? 'checkbox' : 'radio'}" name="question-${q.id}" value="${esc(o.id)}" data-answer="${q.id}" ${selected ? 'checked' : ''} ${disabled} ${invalid} aria-describedby="${describedBy}"><span class="choice-copy"><span class="choice-label">${esc(o.label)}${rec ? '<span class="rec-label">Recommended</span>' : ''}</span>${o.description ? `<span class="choice-description">${esc(o.description)}</span>` : ''}</span></label>`;
    }).join('')}</div>`;
  }
  function question(q, number, conditional) {
    const value = state.values[q.id], errors = showErrors ? C.errors(payload, state) : {}, error = errors[q.id];
    const has = C.answered(q, value), note = q.type === 'text' ? '' : state.comments[q.id] || '', expanded = opened.has(q.id) || !!note;
    const old = previous?.decision?.data?.answers?.find(a => a.question_id === q.id);
    return `<fieldset id="question-${q.id}" class="question ${error ? 'has-error' : ''}" data-question="${q.id}"><legend><span class="question-number">${String(number).padStart(2,'0')}</span><span id="prompt-${q.id}">${esc(q.prompt)}</span><span class="requirement">${q.required ? 'Required' : 'Optional'}</span></legend>
      ${q.description ? `<div class="question-description" id="desc-${q.id}">${md(q.description)}</div>` : ''}
      <div id="hint-${q.id}" class="question-hint"><span>${instructions[q.type]}${q.type === 'multiple_choice' && (q.min_selections || q.max_selections) ? ` · ${q.min_selections ? 'min ' + q.min_selections : ''}${q.min_selections && q.max_selections ? ', ' : ''}${q.max_selections ? 'max ' + q.max_selections : ''}` : ''}</span>${conditional ? `<span class="followup">${ico('corner-down-right')} Follow-up</span>` : ''}${plugin.readonly ? `<span class="answer-state">${has ? 'Answered' : 'Not answered'}</span>` : ''}</div>
      ${controls(q, error)}
      ${q.recommendation?.reason ? `<p class="recommendation">${ico('sparkles')}<span><strong>Agent’s reasoning:</strong> ${esc(q.recommendation.reason)}</span></p>` : ''}
      ${old ? `<details class="previous"><summary>Previous response</summary><p>${esc(C.describe(q, old.answer))}</p>${old.comment ? `<blockquote>${esc(old.comment)}</blockquote>` : ''}</details>` : ''}
      ${error ? `<p class="question-error" id="error-${q.id}">${ico('circle-alert')} ${esc(error)}</p>` : ''}
      <div class="question-actions">${q.type !== 'text' && (!plugin.readonly || note) ? `<button type="button" class="comment-toggle" id="comment-toggle-${q.id}" data-comment-toggle="${q.id}" aria-expanded="${expanded}" aria-controls="comment-wrap-${q.id}">${ico('message-square')} ${expanded ? 'Comment' : 'Add a comment'}</button>` : ''}${clearButton(q, value, note)}</div>
      <div id="comment-wrap-${q.id}" class="comment-wrap" ${expanded ? '' : 'hidden'}><label for="comment-${q.id}">Comment on this question</label><textarea id="comment-${q.id}" class="field question-comment" data-comment="${q.id}" rows="2" placeholder="Add context, a caveat, or a different suggestion…" ${plugin.readonly ? 'disabled' : ''}>${esc(note)}</textarea></div>
    </fieldset>`;
  }
  /* Whatever was put on this question, taken off again: the answer while
     there is one, then the comment, which outlives the answer it qualified
     and would otherwise have no way back out. */
  function clearButton(q, value, note) {
    if (plugin.readonly) return '';
    if (value !== undefined) return `<button type="button" class="clear-answer" id="clear-${q.id}" data-clear="${q.id}">Clear answer</button>`;
    if (note) return `<button type="button" class="clear-answer" id="clear-${q.id}" data-clear="${q.id}" data-clear-comment="1">Clear comment</button>`;
    return '';
  }

  function render() {
    if (!payload) return;
    const scrolls = ['.workspace', '#questions', '.sidebar', '.sidebar nav'].map(selector => {
      const el = app.querySelector(selector);
      return {selector, top:el?.scrollTop || 0, left:el?.scrollLeft || 0};
    });
    const active = document.activeElement;
    const focus = active?.id ? {id:active.id,start:active.selectionStart,end:active.selectionEnd} : null;
    const visible = C.visible(payload, state), all = C.questions(payload), answered = all.filter(q => visible.has(q.id) && C.answered(q,state.values[q.id])).length;
    const invalid = C.errors(payload,state), required = all.filter(q => visible.has(q.id) && q.required && invalid[q.id]).length;
    const shownGroups = payload.groups.filter(g => g.questions.some(q => visible.has(q.id)));
    const count = visible.size;
    let index = 0;
    const html = `<header class="plugin-header feedback-header"><button type="button" class="rail-toggle" data-rail="1">${Wicket.icon(railOpen ? 'panel-left-close' : 'panel-left-open', {size:15, label: railOpen ? 'Hide the group list' : 'Show the group list'})}</button><h1 class="plugin-title">${esc(plugin.gate.title || 'Feedback')}</h1><span class="header-count">${plugin.readonly ? `Read-only · ${esc(plugin.gate.status || 'closed')}` : `<b>${answered}</b> of ${count} answered`}</span></header>
      <div class="workspace"><aside class="sidebar" ${railOpen ? '' : 'hidden'}><div class="sidebar-label">Questions <span>${count}</span></div><nav aria-label="Question groups">${shownGroups.map((g,i) => {
        const qs = g.questions.filter(q => visible.has(q.id)), done = qs.filter(q => C.answered(q,state.values[q.id])).length;
        return `<button type="button" class="group-link ${done === qs.length ? 'complete' : ''}" data-jump="${g.id}"><span class="group-icon">${done === qs.length ? ico('check') : String(i+1).padStart(2,'0')}</span><span>${esc(g.title)}</span><small>${done}/${qs.length}</small></button>`;
      }).join('')}</nav><div class="sidebar-progress"><div class="progress-track"><span style="width:${count ? answered/count*100 : 100}%"></span></div><p>${plugin.readonly ? 'This response has been recorded.' : required ? `${required} required ${required === 1 ? 'answer' : 'answers'} remaining` : Object.keys(invalid).length ? 'Check the response limits' : 'Ready to hand over'}</p><span>${all.length - count ? `${all.length-count} conditional ${all.length-count === 1 ? 'question is' : 'questions are'} hidden.` : 'Add comments to qualify your choices.'}</span></div></aside>
      <div class="questions-scroll" id="questions"><div class="questions-content">
        ${payload.description ? `<div class="request-description">${md(payload.description)}</div>` : ''}
        ${previous ? '<div class="revision-note">Revised request. Previous responses are shown for context; choose your answers for this round.</div>' : ''}
        ${shellErrors.length ? `<div class="validation-banner" role="alert" tabindex="-1">${shellErrors.map(e => `${esc(e.path || 'Response')}: ${esc(e.message)}`).join('<br>')}</div>` : ''}
        ${showErrors && Object.keys(invalid).length ? `<div class="validation-banner" role="alert">${ico('circle-alert')} Complete ${Object.keys(invalid).length} highlighted ${Object.keys(invalid).length === 1 ? 'question' : 'questions'} before handing over.</div>` : ''}
        ${shownGroups.map((g,i) => `<section class="question-group" id="group-${g.id}" aria-labelledby="group-title-${g.id}"><div class="group-heading"><span class="section-index">${String(i+1).padStart(2,'0')}</span><div><h2 id="group-title-${g.id}">${esc(g.title)}</h2>${g.description ? `<div class="group-description">${md(g.description)}</div>` : ''}</div></div>${g.questions.filter(q => visible.has(q.id)).map(q => question(q, ++index, !!q.when || !!g.when)).join('')}</section>`).join('')}
        ${!count ? '<p class="empty">No questions apply to these answers.</p>' : ''}
        <div class="end-note">${ico('check-check')}<span>${plugin.readonly ? 'Only the questions applicable to this response are shown.' : 'Your answers stay in draft until you use Wicket’s hand-over.'}</span></div>
      </div></div></div>`;
    // Keep the desktop and mobile scroll containers mounted. Replacing them
    // resets their position; CSS smooth scrolling then visibly replays the scroll.
    if (app.querySelector('#questions')) {
      const next = document.createElement('template');
      next.innerHTML = html;
      for (const selector of ['.feedback-header', '.sidebar', '#questions']) {
        app.querySelector(selector).replaceChildren(...next.content.querySelector(selector).childNodes);
      }
      app.querySelector('.sidebar').hidden = !railOpen;
    } else app.innerHTML = html;
    for (const {selector, top, left} of scrolls) {
      app.querySelector(selector)?.scrollTo({top, left, behavior:'instant'});
    }
    if (focus) { const el = document.getElementById(focus.id); el?.focus({preventScroll:true}); if (el?.tagName === 'TEXTAREA' && focus.start !== null) el.setSelectionRange(focus.start,focus.end); }
    plugin.status({label: plugin.readonly ? 'Feedback recorded' : Object.keys(invalid).length ? `Complete ${Object.keys(invalid).length} ${Object.keys(invalid).length === 1 ? 'question' : 'questions'}` : `Hand over ${answered} ${answered === 1 ? 'answer' : 'answers'}`});
  }
  function save() {
    shellErrors = [];
    plugin.draft(state,{flush:true});
    if (!composing) render();
  }
  function change(event) {
    if (!payload || plugin.readonly) return;
    const el = event.target, qid = el.dataset.answer;
    if (el.dataset.comment) { state.comments[el.dataset.comment] = el.value; save(); return; }
    if (!qid) return;
    const q = C.questions(payload).find(q => q.id === qid);
    const before = C.visible(payload,state);
    if (q.type === 'text') state.values[qid] = el.value;
    else if (q.type === 'multiple_choice') {
      const selected = new Set(state.values[qid] || []); el.checked ? selected.add(el.value) : selected.delete(el.value);
      state.values[qid] = q.options.filter(o => selected.has(o.id)).map(o => o.id);
    } else if (q.type === 'boolean') state.values[qid] = el.value === 'true';
    else if (q.type === 'checkbox') state.values[qid] = el.checked;
    else state.values[qid] = el.value;
    save();
    const after = C.visible(payload,state), added = [...after].filter(id => !before.has(id)).length, removed = [...before].filter(id => !after.has(id)).length;
    if (added || removed) document.getElementById('announce').textContent = `${added ? `${added} follow-up ${added === 1 ? 'question' : 'questions'} shown. ` : ''}${removed ? `${removed} ${removed === 1 ? 'question' : 'questions'} hidden. Their drafts are saved but will not be submitted.` : ''}`;
  }
  app.addEventListener('input', e => { if (e.target.tagName === 'TEXTAREA') change(e); });
  app.addEventListener('change', e => { if (e.target.tagName === 'INPUT') change(e); });
  app.addEventListener('compositionstart', () => { composing = true; });
  app.addEventListener('compositionend', e => { composing = false; change(e); });
  app.addEventListener('click', e => {
    const button = e.target.closest('button'); if (!button || !payload) return;
    if (button.dataset.rail) { railOpen = !railOpen; plugin.setSetting('rail_open', railOpen); render(); return; }
    if (button.dataset.jump) { document.getElementById('group-'+button.dataset.jump)?.scrollIntoView({behavior:'smooth',block:'start'}); return; }
    const id = button.dataset.commentToggle;
    if (id) { opened.has(id) ? opened.delete(id) : opened.add(id); render(); if (opened.has(id)) document.getElementById('comment-'+id)?.focus({preventScroll:true}); return; }
    if (button.dataset.clear && !plugin.readonly) {
      const id = button.dataset.clear;
      if (button.dataset.clearComment) { delete state.comments[id]; opened.delete(id); } else delete state.values[id];
      save();
      document.getElementById('question-'+id)?.querySelector('input,textarea')?.focus({preventScroll:true});
    }
  });
})();
