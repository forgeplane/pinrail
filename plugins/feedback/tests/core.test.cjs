const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const C = require('../view/feedback-core.js');
const p = require('../fixtures/01-incident.json').payload;
const fresh = () => C.restore(p,null);

test('all fixtures pass semantic validation and the decided fixture round-trips', () => {
  for (const name of fs.readdirSync(path.join(__dirname,'../fixtures'))) {
    const gate = require('../fixtures/'+name); C.validate(gate.payload);
    if (gate.decision) assert.deepEqual(C.decision(gate.payload,C.restore(gate.payload,gate.decision.data)),gate.decision.data);
  }
});
test('recommendations never preselect answers', () => {
  const s=fresh(); assert.equal(Object.keys(s.values).length,0);
  assert.deepEqual([...C.visible(p,s)],['approach','notify','other_context']);
});
test('boolean false satisfies a required question; missing value does not', () => {
  const q=C.questions(p).find(q=>q.id==='notify'); assert(C.answered(q,false)); assert(!C.answered(q,undefined));
});
test('required acknowledgments must be checked', () => {
  const q=C.questions(p).find(q=>q.id==='preserve_logs'); assert(!C.answered(q,false)); assert(C.answered(q,true));
});
test('nested conditions include all/any, contains, equals and answered', () => {
  const s=fresh(); s.values.approach='patch'; s.values.checks=['replay']; s.values.notify=true;
  let shown=C.visible(p,s); assert(shown.has('checks')); assert(shown.has('preserve_logs')); assert(shown.has('replay_details')); assert(shown.has('channels')); assert(!shown.has('message_focus'));
  s.values.channels=['email']; assert(C.visible(p,s).has('message_focus'));
});
test('hidden parents cannot enable descendants through a saved answer', () => {
  const s=fresh(); s.values.approach='rollback'; s.values.checks=['replay'];
  const shown=C.visible(p,s); assert(!shown.has('checks')); assert(!shown.has('replay_details'));
});
test('hidden responses and comments remain in drafts but are excluded from decisions', () => {
  const s=fresh(); Object.assign(s.values,{approach:'rollback',preserve_logs:true,checks:['replay'],replay_details:'stale instructions',notify:false}); s.comments.checks='hidden comment';
  const d=C.decision(p,s); assert(!d.answers.some(a=>a.question_id==='replay_details')); assert(d.excluded.includes('replay_details')); assert.equal(s.comments.checks,'hidden comment');
  s.values.approach='patch'; assert(C.visible(p,s).has('replay_details')); assert.equal(s.values.replay_details,'stale instructions');
});
test('comment-only optional choices are represented explicitly', () => {
  const p=require('../fixtures/03-handoff.json').payload, s=C.restore(p,null);
  Object.assign(s.values,{include_appendix:false,reviewed:true}); s.comments.extra='Only if Finance confirms it is ready.';
  const d=C.decision(p,s); assert.deepEqual(d.answers.at(-1),{question_id:'extra',answer:null,comment:'Only if Finance confirms it is ready.'}); assert(d.unanswered.includes('extra'));
});
test('comments do not satisfy required answers', () => {
  const s=fresh(); s.comments.approach='Some context'; assert.throws(()=>C.decision(p,s));
});
test('selection bounds and nonblank text limits are enforced', () => {
  const s=fresh(); s.values.approach='patch'; s.values.checks=[]; assert(C.errors(p,s).checks);
  s.values.checks=['integration','replay','staging']; assert(!C.errors(p,s).checks);
  s.values.approach='investigate'; s.values.investigation_focus='short'; assert(C.errors(p,s).investigation_focus);
  s.values.investigation_focus='   '; assert(C.errors(p,s).investigation_focus);
});
test('group conditions activate and deactivate entire sections', () => {
  const launch=require('../fixtures/02-launch.json').payload, s=C.restore(launch,null);
  s.values.audience='public'; assert(C.visible(launch,s).has('support'));
  s.values.audience='waitlist'; assert(!C.visible(launch,s).has('support'));
});
test('not_equals requires an actual answer, not an unset value', () => {
  const copy=structuredClone(p); copy.groups[0].questions[3].when={question_id:'approach',operator:'not_equals',value:'rollback'}; C.validate(copy);
  const s=C.restore(copy,null); assert(!C.visible(copy,s).has('investigation_focus')); s.values.approach='patch'; assert(C.visible(copy,s).has('investigation_focus'));
});
test('invalid references, cycles, unknown options and mismatched operators are rejected', () => {
  const conditions=[{question_id:'missing',operator:'answered'},{question_id:'checks',operator:'answered'},{question_id:'approach',operator:'contains',value:'rollback'},{question_id:'approach',operator:'equals',value:'nonexistent'}];
  for (const when of conditions) { const copy=structuredClone(p); copy.groups[0].questions[1].when=when; assert.throws(()=>C.validate(copy)); }
  const copy=structuredClone(p); copy.groups[0].when={question_id:'approach',operator:'answered'}; assert.throws(()=>C.validate(copy));
});
test('stale option IDs are discarded on draft restoration', () => {
  const s=C.restore(p,{values:{approach:'deleted',checks:['replay','deleted']},comments:{approach:'Retain this note'}});
  assert.equal(s.values.approach,undefined); assert.deepEqual(s.values.checks,['replay']); assert.equal(s.comments.approach,'Retain this note');
});
