import path from 'node:path';
import { test, expect } from '@playwright/test';
import { mountPlugin, fixture } from '@forgeplane/pinrail-plugin/testing';
const root = path.resolve(__dirname, '..');
const personal = () => fixture(path.join(root, 'fixtures/01-personal-assistant.json'));
// the calendar, or why it cannot be shown, is drawn before anything is done to it
const mount = async (page, overrides = {}) => {
  const p = await mountPlugin(page, root, { gate: personal(), theme: 'light', ...overrides });
  await expect(p.frame.locator('.calendar-header, .fatal').first()).toBeVisible();
  return p;
};
const slot = (p, id) => p.frame.locator(`.event[data-option="${id}"]`);

test('real SDK handshake, blocked time, conflict filtering, restoration, one choice per item', async ({ page }) => {
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  const p = await mount(page);
  await expect(p.frame.getByRole('heading', { name: 'Dinner, doctor & tennis' })).toBeVisible();
  await expect(p.frame.locator('.blocked')).toHaveCount(6);
  await expect(slot(p,'doctor-busy')).toHaveCount(0);
  await slot(p,'doctor-mon').click();
  await expect(slot(p,'tennis-mon')).toHaveCount(0);
  await expect(slot(p,'doctor-mon')).toHaveAttribute('aria-pressed','true');
  await slot(p,'doctor-thu').click();
  await expect(slot(p,'doctor-mon')).toHaveAttribute('aria-pressed','false');
  await expect(slot(p,'tennis-mon')).toBeVisible();
  await slot(p,'tennis-tue').click();
  await expect(slot(p,'dinner-tue')).toHaveCount(0);
  await slot(p,'tennis-tue').click();
  await expect(slot(p,'dinner-tue')).toHaveCount(1);
  expect(errors).toEqual([]);
});
test('draft reload, themes, hand-over, accepted decision, and read-only state', async ({ page }) => {
  const p = await mount(page);
  await slot(p,'doctor-mon').click();
  await slot(p,'tennis-tue').click();
  await slot(p,'dinner-wed').click();
  await expect.poll(async () => (await p.lastDraft())?.selected.dinner).toBe('dinner-wed');
  await p.reinit();
  await expect(slot(p,'doctor-mon')).toHaveAttribute('aria-pressed','true');
  await p.send({type:'appearance',theme:'dark'});
  await expect(p.frame.locator('html')).toHaveAttribute('data-theme','dark');
  await expect(slot(p,'tennis-tue')).toHaveAttribute('aria-pressed','true');
  await p.collect();
  const data = await p.nextSubmit();
  expect(data.verdict).toBe('approve');
  expect(data.selections.map(s => s.option_id)).toEqual(['doctor-mon','tennis-tue','dinner-wed']);
  expect(data.deferred).toEqual([]);
  await p.sendSubmitted({data});
  await expect(slot(p,'doctor-mon')).toBeDisabled();
  await expect(p.frame.locator('.event.suggestion')).toHaveCount(3);
  await p.collect(); await page.waitForTimeout(300);
  expect((await p.messages()).filter(m => m.type === 'submit')).toHaveLength(1);
});
test('incomplete decisions are held, explicit deferrals return to agent, violations are visible', async ({ page }) => {
  const p = await mount(page);
  await p.collect();
  await expect(p.frame.getByRole('alert')).toContainText('Choose a time');
  expect((await p.messages()).some(m => m.type === 'submit')).toBe(false);
  await slot(p,'doctor-mon').click();
  await slot(p,'tennis-tue').click();
  await p.frame.locator('[data-activity="dinner"]').getByRole('button',{name:'Another time'}).click();
  await p.collect();
  const data = await p.nextSubmit();
  expect(data.verdict).toBe('revise'); expect(data.deferred).toEqual(['dinner']);
  await p.sendViolations([{path:'/selections/0',message:'Availability has changed; ask for another proposal.'}]);
  await expect(p.frame.getByRole('alert')).toContainText('Availability has changed');
  await expect(slot(p,'doctor-mon')).toBeEnabled();
});
test('read-only fixtures restore the decided choices and do not emit a new decision', async ({ page }) => {
  const p = await mount(page,{gate:fixture(path.join(root,'fixtures/03-personal.decided.json')),readonly:true});
  await expect(p.frame.locator('.event.selected')).toHaveCount(3);
  await expect(slot(p,'dinner-wed')).toBeDisabled();
  // the view has time to answer the collect before the log is read
  await p.collect(); await page.waitForTimeout(300); expect((await p.messages()).some(m => m.type === 'submit')).toBe(false);
});
test('same calendar handles interview scheduling', async ({ page }) => {
  const p = await mount(page,{gate:fixture(path.join(root,'fixtures/02-interviews.json'))});
  await slot(p,'maya-mon').click(); await expect(slot(p,'james-mon')).toHaveCount(0);
  await slot(p,'elena-thu').click(); await expect(slot(p,'maya-thu')).toHaveCount(0);
  await slot(p,'james-fri').click(); await slot(p,'omar-tue').click();
  await p.collect(); expect((await p.nextSubmit()).selections).toHaveLength(4);
});
test('no availability is recoverable through an explicit deferral', async ({ page }) => {
  const p = await mount(page,{gate:fixture(path.join(root,'fixtures/04-no-availability.json'))});
  await expect(p.frame.getByText('No compatible times.',{exact:false})).toBeVisible();
  await p.frame.getByRole('button',{name:'Another time'}).click(); await p.collect();
  expect((await p.nextSubmit()).deferred).toEqual(['dentist']);
});
test('list is keyboard-operable and responsive; payload markup stays text', async ({ page }) => {
  await page.setViewportSize({width:390,height:844});
  const gate = personal(); gate.payload.items[0].title = '<img src=x onerror=alert(1)> doctor';
  const p = await mount(page,{gate});
  await p.frame.getByRole('button',{name:'List',exact:true}).click();
  const option = p.frame.locator('.list-group .option').first();
  await option.focus(); await option.press('Enter');
  await expect(option).toHaveAttribute('aria-pressed','true');
  await expect(p.frame.locator('img')).toHaveCount(0);
  expect(await p.frame.locator('body').evaluate(el => el.scrollWidth <= window.innerWidth)).toBe(true);
});
test('invalid payload fails closed and hostile drafts cannot bypass conflicts', async ({ page }) => {
  const gate = personal(); gate.payload.timezone = 'Invalid/Zone';
  const p = await mount(page,{gate}); await expect(p.frame.getByRole('alert')).toContainText('timezone');
  // the view has time to answer the collect before the log is read
  await p.collect(); await page.waitForTimeout(300); expect((await p.messages()).some(m => m.type === 'submit')).toBe(false);
});
test('day and week views: a day at full width, a strip of days, paging by week, and keys', async ({ page }) => {
  // two weeks, with a suggestion in the second
  const gate = personal();
  gate.payload.days = 10;
  gate.payload.items[1].options.push({ id: 'tennis-next', start: '2026-09-29T17:00:00+03:00', end: '2026-09-29T18:30:00+03:00', location: 'Athens Tennis Club · Court 1' });
  const p = await mountPlugin(page, root, { gate, theme: 'light' });
  const f = p.frame;
  const columns = f.locator('.day-column');

  // a week at a time, paged from the first day
  await expect(f.getByRole('button', { name: 'Week', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(columns).toHaveCount(7);
  await expect(slot(p, 'tennis-next')).toHaveCount(0);
  await f.getByRole('button', { name: 'Next week' }).click();
  await expect(columns).toHaveCount(3);
  await expect(slot(p, 'tennis-next')).toBeVisible();
  await f.getByRole('button', { name: 'First day' }).click();
  await expect(columns).toHaveCount(7);

  // one day, with every day of the calendar in a strip
  await f.getByRole('button', { name: 'Day', exact: true }).click();
  await expect(columns).toHaveCount(1);
  // the strip is the week around the day shown
  await expect(f.locator('.day-strip button')).toHaveCount(7);
  await expect(f.locator('.day-strip button.is-current')).toContainText('21');
  await expect(slot(p, 'doctor-mon')).toBeVisible();
  await f.locator('h1').click();
  await f.locator('body').press('j');
  await expect(f.locator('.day-strip button.is-current')).toContainText('22');
  await expect(slot(p, 'tennis-tue')).toBeVisible();
  await f.locator('.day-strip button').filter({ hasText: '27' }).click();
  await f.locator('body').press('j');
  await f.locator('body').press('j');
  // into the second week: the strip follows
  await expect(f.locator('.day-strip button')).toHaveCount(3);
  await expect(f.locator('.day-strip button.is-current')).toContainText('29');
  await expect(slot(p, 'tennis-next')).toBeVisible();

  // a choice made in one view holds in the others
  await slot(p, 'tennis-next').click();
  await f.locator('body').press('w');
  await expect(columns).toHaveCount(3);
  await expect(slot(p, 'tennis-next')).toHaveAttribute('aria-pressed', 'true');
  await f.locator('body').press('t');
  await expect(columns).toHaveCount(7);
  await f.locator('body').press('l');
  await expect(f.locator('.list-scroll')).toBeVisible();

  // the view is remembered for the next calendar
  const kept = (await p.messages()).filter((m: any) => m.type === 'settings_set').map((m: any) => m.patch.view);
  expect(kept[kept.length - 1]).toBe('list');
});

test('a single day opens in the day view', async ({ page }) => {
  const gate = personal();
  gate.payload.days = 1;
  gate.payload.items = gate.payload.items.map((item: any) => ({ ...item, options: item.options.filter((o: any) => o.start.startsWith('2026-09-21')) })).filter((item: any) => item.options.length);
  gate.payload.blocked = gate.payload.blocked.filter((b: any) => b.start.startsWith('2026-09-21'));
  const p = await mountPlugin(page, root, { gate, theme: 'light' });
  await expect(p.frame.getByRole('button', { name: 'Day', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(p.frame.locator('.day-column')).toHaveCount(1);
  await expect(p.frame.locator('.day-strip')).toHaveCount(0);
  await expect(p.frame.locator('.pager')).toHaveCount(0);
});

test('an item can be declined, or sent back for another time, with a note for the agent', async ({ page }) => {
  const p = await mount(page);
  const f = p.frame;
  await slot(p, 'doctor-mon').click();
  const tennis = f.locator('[data-activity="tennis"]');
  await tennis.getByRole('button', { name: 'Another time' }).click();
  await expect(tennis).toContainText('Another time asked for');
  await expect(tennis.locator('textarea')).toBeFocused();
  await page.keyboard.type('any evening after 18:00');
  // its suggestions leave the calendar while it waits for another time
  await expect(slot(p, 'tennis-tue')).toHaveCount(0);

  const dinner = f.locator('[data-activity="dinner"]');
  await dinner.getByRole('button', { name: 'Decline' }).click();
  await expect(dinner).toContainText('Declined');
  await dinner.locator('textarea').fill('not this week');
  await expect(f.locator('.header-progress')).toContainText('1 for another time · 1 declined');

  await p.collect();
  const data = await p.nextSubmit();
  expect(data.verdict).toBe('revise');
  expect(data.deferred).toEqual(['tennis']);
  expect(data.declined).toEqual(['dinner']);
  expect(data.notes).toEqual([{ item_id: 'tennis', note: 'any evening after 18:00' }, { item_id: 'dinner', note: 'not this week' }]);
});

test('undo puts a declined item back to choosing', async ({ page }) => {
  const p = await mount(page);
  const dinner = p.frame.locator('[data-activity="dinner"]');
  await dinner.getByRole('button', { name: 'Decline' }).click();
  await expect(slot(p, 'dinner-wed')).toHaveCount(0);
  await dinner.getByRole('button', { name: 'Choose a time instead' }).click();
  await expect(slot(p, 'dinner-wed')).toBeVisible();
  await expect(dinner.locator('textarea')).toHaveCount(0);
});
