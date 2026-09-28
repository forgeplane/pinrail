/* global FeedbackCore -- feedback-core.js, loaded before this file */
(function () {
  'use strict';
  const C = FeedbackCore, esc = Pinrail.escape, md = Pinrail.markdown, ico = name => Pinrail.icon(name, {size: 14});
  const app = document.getElementById('app');
  let payload, state, previous, showErrors = false, shellErrors = [], opened = new Set(), composing = false;
  // the rail of groups: shown by default, and the choice is the shell's to
  // keep, so it holds for the next set of questions too
  let railOpen = true;
  // the question j and k move from: the last one moved to, clicked or typed in
  let current = null;
  const plugin = Pinrail.connect({
    resize: 'fill',
    onInit({review, draft, previous: old, settings}) {
      applySettings(settings);
      showErrors = false; shellErrors = []; previous = old;
      try { payload = C.validate(review.payload); state = C.restore(payload, review.decision?.data || draft); opened = withComments(); }
      catch (e) { payload = null; app.innerHTML = `<div class="fatal" role="alert"><h1>Unable to show these questions</h1><p>${esc(e.message)}</p></div>`; plugin.status({label:'Questions need correction'}); return; }
      render();
    },
    onCollect() {
      if (!payload || plugin.readonly) return;
      try { plugin.submit(C.decision(payload, state)); }
      catch { showErrors = true; render(); const first = app.querySelector('[aria-invalid="true"]'); first?.focus(); first?.scrollIntoView({block:'center'}); }
    },
    onViolations(errors) { shellErrors = errors; render(); app.querySelector('[role="alert"]')?.focus(); },
    onSubmitted() { if (!payload) return; state = C.restore(payload, plugin.review.decision?.data); opened = withComments(); shellErrors = []; showErrors = false; render(); },
    onSettings(settings) { applySettings(settings); render(); },
  });
  // a comment already written starts open; after that, open is the person's call
  function withComments() { return new Set(Object.keys(state.comments).filter(id => state.comments[id])); }
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
    return `<div class="choices ${q.type === 'boolean' ? 'boolean-choices' : options.some(o => o.description) ? 'described' : ''}">${options.map((o, index) => {
      const optionValue = q.type === 'boolean' ? o.id === 'true' : o.id;
      const selected = multiple ? (value || []).includes(o.id) : value === optionValue;
      const rec = q.recommendation && (multiple ? q.recommendation.answer.includes(o.id) : q.recommendation.answer === optionValue);
      return `<label class="choice ${selected ? 'is-selected' : ''}"><input id="answer-${q.id}-${index}" type="${multiple ? 'checkbox' : 'radio'}" name="question-${q.id}" value="${esc(o.id)}" data-answer="${q.id}" ${selected ? 'checked' : ''} ${disabled} ${invalid} aria-describedby="${describedBy}"><span class="choice-copy"><span class="choice-label">${esc(o.label)}${rec ? '<span class="rec-label">Recommended</span>' : ''}</span>${o.description ? `<span class="choice-description">${esc(o.description)}</span>` : ''}</span></label>`;
    }).join('')}</div>`;
  }
  function question(q, number, conditional) {
    const value = state.values[q.id], errors = showErrors ? C.errors(payload, state) : {}, error = errors[q.id];
    const has = C.answered(q, value), note = q.type === 'text' ? '' : state.comments[q.id] || '', expanded = opened.has(q.id);
    const old = previous?.decision?.data?.answers?.find(a => a.question_id === q.id);
    const rec = q.recommendation, taken = rec && JSON.stringify(rec.answer) === JSON.stringify(value);
    const status = error ? 'error' : has ? 'answered' : q.required ? 'required' : 'open';
    // what the agent would pick, above the options; the last round and the comment below them
    const agent = rec ? `<div class="agent-card ${taken ? 'is-taken' : ''}"><div class="agent-copy"><div class="agent-head">${ico('sparkles')}<span>Agent recommends</span><strong class="agent-answer">${esc(C.describe(q, rec.answer))}</strong></div>${rec.reason ? `<p class="agent-reason">${esc(rec.reason)}</p>` : ''}</div>${plugin.readonly ? '' : taken ? `<span class="agent-taken">${ico('check')} Your answer</span>` : `<button type="button" class="use-rec" data-use-rec="${q.id}">${ico('corner-down-left')} Use this answer</button>`}</div>` : '';
    const after = [
      old ? `<details class="previous"><summary>Previous response</summary><p>${esc(C.describe(q, old.answer))}</p>${old.comment ? `<blockquote>${esc(old.comment)}</blockquote>` : ''}</details>` : '',
      q.type !== 'text' && (!plugin.readonly || note) ? `<div class="question-actions">${commentToggle(q, note, expanded)}</div><div id="comment-wrap-${q.id}" class="comment-wrap" ${expanded ? '' : 'hidden'}><div class="comment-head"><label for="comment-${q.id}">Comment on this question</label>${note && !plugin.readonly ? `<button type="button" class="remove-comment" id="remove-comment-${q.id}" data-remove-comment="${q.id}">${ico('trash-2')} Remove comment</button>` : ''}</div><textarea id="comment-${q.id}" class="field question-comment" data-comment="${q.id}" rows="3" placeholder="Add context, a caveat, or a different suggestion…" ${plugin.readonly ? 'disabled' : ''}>${esc(note)}</textarea></div>` : '',
    ].join('');
    return `<fieldset id="question-${q.id}" class="question is-${status} ${current === q.id ? 'is-current' : ''}" data-question="${q.id}"><legend><span class="question-number" aria-hidden="true">${has ? Pinrail.icon('check', {size: 12}) : String(number).padStart(2,'0')}</span><span id="prompt-${q.id}" class="question-prompt">${esc(q.prompt)}</span><span class="question-meta">${value !== undefined && !plugin.readonly ? `<button type="button" class="clear-answer" id="clear-${q.id}" data-clear="${q.id}">${Pinrail.icon('rotate-ccw', {size: 11})} Clear answer</button>` : ''}<span class="requirement ${q.required ? 'is-required' : ''}">${q.required ? 'Required' : 'Optional'}</span></span></legend>
      <div class="question-body">
        ${q.description ? `<div class="question-description" id="desc-${q.id}">${md(q.description)}</div>` : ''}
        ${agent}
        <div id="hint-${q.id}" class="question-hint"><span>${instructions[q.type]}${q.type === 'multiple_choice' && (q.min_selections || q.max_selections) ? ` · ${q.min_selections ? 'min ' + q.min_selections : ''}${q.min_selections && q.max_selections ? ', ' : ''}${q.max_selections ? 'max ' + q.max_selections : ''}` : ''}</span>${conditional ? `<span class="followup">${ico('corner-down-right')} Follow-up</span>` : ''}${plugin.readonly ? `<span class="answer-state">${has ? 'Answered' : 'Not answered'}</span>` : ''}</div>
        ${controls(q, error)}
        ${error ? `<p class="question-error" id="error-${q.id}">${ico('circle-alert')} ${esc(error)}</p>` : ''}
        ${after}
      </div>
    </fieldset>`;
  }
  /* Open, it folds the comment away; folded, it shows the start of what was
     written so the comment is not lost from sight. */
  function commentToggle(q, note, expanded) {
    const label = expanded ? `${ico('chevron-up')} Hide comment` : note ? `${ico('message-square')} Comment <span class="comment-preview">${esc(note)}</span>` : `${ico('message-square-plus')} Add a comment`;
    return `<button type="button" class="comment-toggle ${note ? 'has-comment' : ''}" id="comment-toggle-${q.id}" data-comment-toggle="${q.id}" aria-expanded="${expanded}" aria-controls="comment-wrap-${q.id}">${label}</button>`;
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
    const comments = all.filter(q => visible.has(q.id) && q.type !== 'text' && (state.comments[q.id] || '').trim()).length;
    let index = 0;
    const html = `<header class="plugin-header feedback-header"><button type="button" class="rail-toggle" data-rail="1">${Pinrail.icon(railOpen ? 'panel-left-close' : 'panel-left-open', {size:15, label: railOpen ? 'Hide the group list' : 'Show the group list'})}</button><h1 class="plugin-title">${esc(plugin.review.title || 'Feedback')}</h1><span class="header-count">${plugin.readonly ? `Read-only · ${esc(plugin.review.status || 'closed')}` : `<span><b>${answered}</b> of ${count} answered</span>${required ? `<span class="tally-required"><b>${required}</b> required left</span>` : ''}${comments ? `<span><b>${comments}</b> ${comments === 1 ? 'comment' : 'comments'}</span>` : ''}`}</span><div class="header-progress" aria-hidden="true"><span style="width:${count ? answered/count*100 : 100}%"></span></div></header>
      <div class="workspace"><aside class="sidebar" ${railOpen ? '' : 'hidden'}><div class="sidebar-label">Questions <span>${count}</span></div><nav aria-label="Question groups">${shownGroups.map((g,i) => {
        const qs = g.questions.filter(q => visible.has(q.id)), done = qs.filter(q => C.answered(q,state.values[q.id])).length;
        // the group, then each of its questions with where it stands
        return `<div class="rail-group"><button type="button" class="group-link ${done === qs.length ? 'complete' : ''}" data-jump="${g.id}"><span class="group-icon">${done === qs.length ? ico('check') : String(i+1).padStart(2,'0')}</span><span class="group-name">${esc(g.title)}</span><small>${done}/${qs.length}</small></button><ul class="rail-questions">${qs.map(q => {
          const st = showErrors && invalid[q.id] ? 'error' : C.answered(q, state.values[q.id]) ? 'answered' : q.required ? 'required' : 'open';
          return `<li><button type="button" class="rail-question is-${st}" data-jump-question="${q.id}" tabindex="-1"><span class="rail-dot"></span><span class="rail-prompt">${esc(q.prompt)}</span></button></li>`;
        }).join('')}</ul></div>`;
      }).join('')}</nav><div class="sidebar-progress"><div class="progress-track"><span style="width:${count ? answered/count*100 : 100}%"></span></div><p>${plugin.readonly ? 'This response has been recorded.' : required ? `${required} required ${required === 1 ? 'answer' : 'answers'} remaining` : Object.keys(invalid).length ? 'Check the response limits' : 'Ready to hand over'}</p><span>${all.length - count ? `${all.length-count} conditional ${all.length-count === 1 ? 'question is' : 'questions are'} hidden.` : 'Add comments to qualify your choices.'}</span></div></aside>
      <div class="questions-scroll" id="questions"><div class="questions-content">
        ${payload.description ? `<div class="request-description">${md(payload.description)}</div>` : ''}
        ${previous ? '<div class="revision-note">Revised request. Previous responses are shown for context; choose your answers for this round.</div>' : ''}
        ${shellErrors.length ? `<div class="validation-banner" role="alert" tabindex="-1">${shellErrors.map(e => `${esc(e.path || 'Response')}: ${esc(e.message)}`).join('<br>')}</div>` : ''}
        ${showErrors && Object.keys(invalid).length ? `<div class="validation-banner" role="alert">${ico('circle-alert')} Complete ${Object.keys(invalid).length} highlighted ${Object.keys(invalid).length === 1 ? 'question' : 'questions'} before handing over.</div>` : ''}
        ${shownGroups.map((g,i) => `<section class="question-group" id="group-${g.id}" aria-labelledby="group-title-${g.id}"><div class="group-heading"><span class="section-index">${String(i+1).padStart(2,'0')}</span><div><h2 id="group-title-${g.id}">${esc(g.title)}</h2>${g.description ? `<div class="group-description">${md(g.description)}</div>` : ''}</div></div>${g.questions.filter(q => visible.has(q.id)).map(q => question(q, ++index, !!q.when || !!g.when)).join('')}</section>`).join('')}
        ${!count ? '<p class="empty">No questions apply to these answers.</p>' : ''}
        <div class="end-note">${ico('check-check')}<span>${plugin.readonly ? 'Only the questions applicable to this response are shown.' : 'Your answers stay in draft until you use Pinrail’s hand-over.'}</span></div>
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
      const selected = new Set(state.values[qid] || []); if (el.checked) selected.add(el.value); else selected.delete(el.value);
      state.values[qid] = q.options.filter(o => selected.has(o.id)).map(o => o.id);
    } else if (q.type === 'boolean') state.values[qid] = el.value === 'true';
    else if (q.type === 'checkbox') state.values[qid] = el.checked;
    else state.values[qid] = el.value;
    save();
    const after = C.visible(payload,state), added = [...after].filter(id => !before.has(id)).length, removed = [...before].filter(id => !after.has(id)).length;
    if (added || removed) document.getElementById('announce').textContent = `${added ? `${added} follow-up ${added === 1 ? 'question' : 'questions'} shown. ` : ''}${removed ? `${removed} ${removed === 1 ? 'question' : 'questions'} hidden. Their drafts are saved but will not be submitted.` : ''}`;
  }
  /* j and k: the next and previous question, scrolled to, with its answer
     focused so the arrow keys or space answer it. Not while typing. */
  function mark(id) {
    current = id;
    for (const el of app.querySelectorAll('.question.is-current')) el.classList.remove('is-current');
    document.getElementById('question-'+id)?.classList.add('is-current');
  }
  function step(by) {
    const ids = [...app.querySelectorAll('.question[data-question]')].map(el => el.dataset.question);
    if (!ids.length) return;
    const at = ids.indexOf(current);
    const next = ids[at < 0 ? (by > 0 ? 0 : ids.length - 1) : Math.min(ids.length - 1, Math.max(0, at + by))];
    mark(next);
    const el = document.getElementById('question-'+next);
    el.scrollIntoView({behavior:'smooth', block:'start'});
    (el.querySelector('input:checked') || el.querySelector('input, textarea'))?.focus({preventScroll:true});
  }
  document.addEventListener('keydown', e => {
    if (!payload || e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.target.closest?.('textarea, input[type="text"]')) return;
    const key = e.key.toLowerCase();
    if (key === 'j') step(1); else if (key === 'k') step(-1); else return;
    e.preventDefault();
  });
  app.addEventListener('focusin', e => { const q = e.target.closest?.('.question'); if (q && q.dataset.question !== current) mark(q.dataset.question); });
  app.addEventListener('input', e => { if (e.target.tagName === 'TEXTAREA') change(e); });
  app.addEventListener('change', e => { if (e.target.tagName === 'INPUT') change(e); });
  app.addEventListener('compositionstart', () => { composing = true; });
  app.addEventListener('compositionend', e => { composing = false; change(e); });
  app.addEventListener('click', e => {
    const button = e.target.closest('button'); if (!button || !payload) return;
    if (button.dataset.rail) { railOpen = !railOpen; plugin.setSetting('rail_open', railOpen); render(); return; }
    if (button.dataset.jump) { document.getElementById('group-'+button.dataset.jump)?.scrollIntoView({behavior:'smooth',block:'start'}); return; }
    if (button.dataset.jumpQuestion) { document.getElementById('question-'+button.dataset.jumpQuestion)?.scrollIntoView({behavior:'smooth',block:'start'}); return; }
    const id = button.dataset.commentToggle;
    if (id) { if (opened.has(id)) opened.delete(id); else opened.add(id); render(); if (opened.has(id)) document.getElementById('comment-'+id)?.focus({preventScroll:true}); return; }
    if (plugin.readonly) return;
    if (button.dataset.removeComment) {
      const id = button.dataset.removeComment;
      delete state.comments[id]; opened.delete(id);
      save();
      document.getElementById('comment-toggle-'+id)?.focus({preventScroll:true});
    }
    if (button.dataset.useRec) {
      const q = C.questions(payload).find(q => q.id === button.dataset.useRec);
      state.values[q.id] = structuredClone(q.recommendation.answer);
      save();
      document.getElementById('question-'+q.id)?.querySelector('input:checked,textarea')?.focus({preventScroll:true});
      return;
    }
    if (button.dataset.clear) {
      const id = button.dataset.clear;
      delete state.values[id];
      save();
      document.getElementById('question-'+id)?.querySelector('input,textarea')?.focus({preventScroll:true});
    }
  });
})();
