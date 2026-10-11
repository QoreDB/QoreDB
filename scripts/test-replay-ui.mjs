// SPDX-License-Identifier: BUSL-1.1

import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';

const { chromium } = await import(process.env.QOREDB_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({ headless: true, executablePath: process.env.QOREDB_CHROMIUM_EXECUTABLE });
const page = await browser.newPage({ viewport: { width: 700, height: 600 } });
const errors = [];
page.on('pageerror', error => errors.push(error.message));
await page.route('**/src/lib/notify.tsx*', route => route.fulfill({
  contentType: 'application/javascript',
  body: 'export const notify = {error: (...args) => window.__notices.push(args), success: () => {}};',
}));
await page.addInitScript(() => {
  window.__notices = [];
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  window.__ipc = { calls: [], errorReport: false, hold: [], pending: {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 1,
    invoke: async (command, args) => {
      window.__ipc.calls.push({ command, args });
      const slug = args?.slug ?? args?.request?.slug ?? '';
      const key = command + ':' + slug;
      if (window.__ipc.hold.includes(key)) return new Promise((resolve, reject) => {
        window.__ipc.pending[key] = { resolve, reject };
      });
      if (command === 'plugin:event|listen') return 1;
      if (command === 'plugin:event|unlisten') return null;
      if (command === 'replay_recording_status') return null;
      if (['replay_list_sets', 'replay_list_runs', 'list_sessions'].includes(command)) return [];
      if (command === 'replay_load_set') return { name: args.slug, entries: [] };
      if (command === 'replay_run') return { run: { run_id: slug }, results: [], summary: {} };
      if (command === 'replay_cancel_run') return null;
      if (command === 'replay_last_report') {
        if (window.__ipc.errorReport) throw new Error('Synthetic corrupt report');
        return { report: { run: { run_id: args.slug }, results: [], summary: {} } };
      }
      throw new Error('Unexpected Replay IPC: ' + command);
    },
  };
  window.$RefreshReg$ = () => {};
  window.$RefreshSig$ = () => type => type;
});
const base = process.env.QOREDB_UI_BASE_URL ?? 'http://127.0.0.1:1430';
try {
  await page.goto(new URL('/scripts/fixtures/replay/index.html?lang=en', base).href);
  await page.getByRole('status').filter({ hasText: 'Replay cancelled.' }).waitFor();
  await page.getByText('1/2 results compared', { exact: false }).waitFor();
  console.log('PASS cancelled report preserves actual coverage and warns about in-flight queries');
  await page.getByRole('button', { name: 'Complete', exact: true }).click();
  await page.getByText('2/2 results compared', { exact: false }).waitFor();
  assert.equal(await page.getByRole('status').count(), 0);
  console.log('PASS complete report has no cancellation warning');
  await page.goto(new URL('/scripts/fixtures/replay/index.html?lang=fr', base).href);
  await page.getByRole('status').filter({ hasText: 'Rejeu annulé.' }).waitFor();
  await mkdir('.perf', { recursive: true });
  await page.screenshot({ path: '.perf/replay-cancelled.png' });
  console.log('PASS French cancelled report at 700px');
  await page.goto(new URL('/scripts/fixtures/replay/hook.html?lang=en', base).href);
  await page.waitForFunction(() => Boolean(window.__replay));
  await page.evaluate(() => window.__replay.selectSet('valid'));
  await page.waitForFunction(() => window.__replay.report?.run.run_id === 'valid');
  await page.evaluate(async () => {
    window.__ipc.errorReport = true;
    await window.__replay.selectSet('broken');
  });
  assert.equal(await page.evaluate(() => window.__notices.some(args => args.join(' ').includes('Synthetic corrupt report'))), true);
  assert.equal(await page.evaluate(() => window.__replay.report), null);
  console.log('PASS corrupt stored report surfaces its error and clears previous results');
  assert.deepEqual(errors, []);
  const failures = [];
  async function freshHook() {
    await page.goto(new URL('/scripts/fixtures/replay/hook.html?lang=en', base).href);
    await page.waitForFunction(() => Boolean(window.__replay));
  }
  async function scenario(name, action) {
    await freshHook();
    try { await action(); console.log('PASS ' + name); }
    catch (error) { failures.push(name + ': ' + error.message); console.log('FAIL ' + name); }
  }
  await scenario('late set load cannot replace the latest selection', async () => {
    await page.evaluate(() => {
      window.__ipc.hold = ['replay_load_set:a'];
      void window.__replay.selectSet('a');
    });
    await page.evaluate(() => window.__replay.selectSet('b'));
    await page.evaluate(() => window.__ipc.pending['replay_load_set:a'].resolve({ name: 'a', entries: [] }));
    await page.waitForTimeout(50);
    assert.equal(await page.evaluate(() => window.__replay.activeSlug), 'b');
    assert.equal(await page.evaluate(() => window.__ipc.calls.some(c => c.command === 'replay_last_report' && c.args.slug === 'a')), false);
  });
  await scenario('late report cannot replace the latest selection', async () => {
    await page.evaluate(() => {
      window.__ipc.hold = ['replay_last_report:a'];
      void window.__replay.selectSet('a');
    });
    await page.waitForFunction(() => window.__ipc.pending['replay_last_report:a']);
    await page.evaluate(() => window.__replay.selectSet('b'));
    await page.evaluate(() => window.__ipc.pending['replay_last_report:a'].resolve({ report: { run: { run_id: 'a' } } }));
    await page.waitForTimeout(50);
    assert.equal(await page.evaluate(() => window.__replay.report?.run.run_id), 'b');
  });
  await scenario('late replay result cannot replace another set report', async () => {
    await page.evaluate(() => window.__replay.selectSet('a'));
    await page.evaluate(() => { window.__ipc.hold = ['replay_run:a']; void window.__replay.replay(); });
    await page.evaluate(() => window.__replay.selectSet('b'));
    await page.evaluate(() => window.__ipc.pending['replay_run:a'].resolve({ run: { run_id: 'late-a' } }));
    await page.waitForTimeout(50);
    assert.equal(await page.evaluate(() => window.__replay.report?.run.run_id), 'b');
  });
  await scenario('same-tick duplicate replay is dispatched once', async () => {
    await page.evaluate(() => window.__replay.selectSet('a'));
    await page.evaluate(() => {
      window.__ipc.hold = ['replay_run:a'];
      void window.__replay.replay(); void window.__replay.replay();
    });
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'replay_run').length), 1);
  });
  await scenario('connection change clears results and ignores the old run', async () => {
    await page.evaluate(() => window.__replay.selectSet('a'));
    await page.evaluate(() => { window.__ipc.hold = ['replay_run:a']; void window.__replay.replay(); });
    await page.evaluate(() => window.__switchContext('another-session'));
    await page.waitForTimeout(50);
    assert.equal(await page.evaluate(() => window.__replay.report), null);
    await page.evaluate(() => window.__ipc.pending['replay_run:a'].resolve({ run: { run_id: 'late-a' } }));
    await page.waitForTimeout(50);
    assert.equal(await page.evaluate(() => window.__replay.report), null);
  });
  await scenario('workspace change discards a pending selection', async () => {
    await page.evaluate(() => { window.__ipc.hold = ['replay_load_set:a']; void window.__replay.selectSet('a'); });
    await page.evaluate(() => window.__switchContext('synthetic-session', 'other-project'));
    await page.waitForTimeout(50);
    await page.evaluate(() => window.__ipc.pending['replay_load_set:a'].resolve({ name: 'a', entries: [] }));
    await page.waitForTimeout(50);
    assert.equal(await page.evaluate(() => window.__replay.activeSlug), null);
    assert.equal(await page.evaluate(() => window.__replay.report), null);
  });
  await scenario('obsolete selection errors do not interrupt the new context', async () => {
    await page.evaluate(() => { window.__ipc.hold = ['replay_load_set:a']; void window.__replay.selectSet('a'); });
    await page.evaluate(() => window.__replay.selectSet('b'));
    await page.evaluate(() => window.__ipc.pending['replay_load_set:a'].reject('Synthetic obsolete error'));
    await page.waitForTimeout(50);
    assert.equal(await page.evaluate(() => window.__notices.length), 0);
  });
  for (const [label, command, action] of [
    ['late A/B result', 'replay_run_ab', 'replayAb'],
    ['late accepted reference', 'replay_accept_run', 'acceptRun'],
    ['late ignored-columns update', 'replay_set_ignored_columns', 'updateIgnoredColumns'],
    ['late deletion', 'replay_delete_set', 'removeSet'],
  ]) {
    await scenario(label + ' cannot overwrite another selected set', async () => {
      await page.evaluate(() => window.__replay.selectSet('a'));
      await page.evaluate(({ command, action }) => {
        window.__ipc.hold = [command + ':a'];
        void window.__replay[action](action === 'updateIgnoredColumns' ? ['id'] : 'a');
      }, { command, action });
      await page.evaluate(() => window.__replay.selectSet('b'));
      await page.evaluate(command => window.__ipc.pending[command + ':a'].resolve({ name: 'a', right: { run_id: 'late-a' } }), command);
      await page.waitForTimeout(50);
      assert.equal(await page.evaluate(() => window.__replay.activeSlug), 'b');
      assert.equal(await page.evaluate(() => window.__replay.activeSet.name), 'b');
      assert.equal(await page.evaluate(() => window.__replay.report?.run.run_id), 'b');
      assert.equal(await page.evaluate(() => window.__replay.abReport), null);
    });
  }
  await scenario('a callback from an obsolete selection cannot dispatch mutations', async () => {
    await page.evaluate(async () => { await window.__replay.selectSet('a'); });
    await page.evaluate(() => { window.__oldReplay = window.__replay; });
    await page.evaluate(() => window.__replay.selectSet('b'));
    await page.evaluate(async () => { await window.__oldReplay.acceptRun('old-run'); await window.__oldReplay.updateIgnoredColumns(['id']); });
    assert.equal(await page.evaluate(() => window.__ipc.calls.some(c => ['replay_accept_run', 'replay_set_ignored_columns'].includes(c.command))), false);
  });
  await scenario('a stored report loaded late cannot overwrite a freshly completed replay', async () => {
    await page.evaluate(() => { window.__ipc.hold = ['replay_last_report:a']; void window.__replay.selectSet('a'); });
    await page.waitForFunction(() => window.__replay.activeSlug === 'a');
    await page.evaluate(() => window.__replay.replay());
    await page.evaluate(() => window.__ipc.pending['replay_last_report:a'].resolve({ report: { run: { run_id: 'old-stored' } } }));
    await page.waitForTimeout(50);
    assert.equal(await page.evaluate(() => window.__replay.report?.run.run_id), 'a');
  });
  assert.deepEqual(failures, []);
  assert.deepEqual(errors, []);
  console.log('17 Replay scenarios passed (synthetic reports and simulated IPC).');
} finally {
  await browser.close();
}
