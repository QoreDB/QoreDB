// SPDX-License-Identifier: BUSL-1.1

import assert from 'node:assert/strict';

const { chromium } = await import(process.env.QOREDB_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({ headless: true, executablePath: process.env.QOREDB_CHROMIUM_EXECUTABLE });
const page = await browser.newPage();
page.setDefaultTimeout(3000);
const errors = [];
const failures = [];
page.on('pageerror', error => errors.push(error.message));
await page.route('**/src/providers/WorkspaceProvider.tsx*', route => route.fulfill({
  contentType: 'application/javascript',
  body: 'export const useWorkspace = () => ({projectId:window.__workspace});',
}));
await page.addInitScript(() => {
  window.$RefreshReg$ = () => {};
  window.$RefreshSig$ = () => type => type;
  window.__workspace = 'fixture';
  window.__ipc = { calls: [], pending: [], hold: false, value: 'same', nextSession: 0 };
  window.__result = value => ({
    columns: [{ name: 'id', data_type: 'integer', nullable: false }, { name: 'name', data_type: 'text', nullable: false }],
    rows: [{ values: [1, value] }], execution_time_ms: 0,
  });
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 1,
    invoke: async (command, args) => {
      window.__ipc.calls.push({ command, args });
      if (command === 'connect_saved_connection') {
        if (window.__ipc.holdConnect) return new Promise(resolve => window.__ipc.pending.push({ command, args, resolve }));
        return { success: true, session_id: 'session-' + (++window.__ipc.nextSession) };
      }
      if (command === 'disconnect') return { success: true };
      if (command === 'list_namespaces') return window.__ipc.failNamespaces
        ? { success: false, error: 'synthetic namespace failure' }
        : { success: true, namespaces: [{ database: 'fixture', schema: 'public' }] };
      if (command === 'describe_table') return { success: false };
      if (command === 'get_snapshot') {
        if (window.__ipc.hold) return new Promise(resolve => window.__ipc.pending.push({ command, args, resolve }));
        await new Promise(resolve => setTimeout(resolve, 20));
        return { success: false, error: 'synthetic missing snapshot' };
      }
      if (command === 'execute_query' || command === 'preview_table') {
        if (window.__ipc.hold) return new Promise(resolve => window.__ipc.pending.push({ command, args, resolve }));
        if (window.__ipc.rejectLeft && args.query === 'SELECT left') return { success: false, error: 'synthetic query failure' };
        return { success: true, truncated: Boolean(window.__ipc.truncated), result: window.__result(args.query === 'SELECT left' ? window.__ipc.value : 'same') };
      }
      throw new Error('Unexpected fixture IPC: ' + command);
    },
  };
});
const url = new URL('/scripts/fixtures/pro-workflows/hook.html?lang=en', process.env.QOREDB_UI_BASE_URL ?? 'http://127.0.0.1:1430').href;
async function fresh() {
  await page.goto(url);
  await page.waitForFunction(() => window.__sources?.leftSource.sessionId && window.__sources?.rightSource.sessionId);
}
async function check(name, run) {
  try {
    await fresh();
    await run();
    console.log('PASS ' + name);
  } catch (error) {
    failures.push(name + ': ' + error.message);
    console.log('FAIL ' + name + ': ' + error.message);
  }
}
try {
  for (const success of [true, false]) {
    await check('late ' + (success ? 'success' : 'failure') + ' cannot restore a changed query', async () => {
      await page.evaluate(() => { window.__ipc.hold = true; void window.__sources.executeLeft(); });
      await page.waitForFunction(() => window.__ipc.pending.length === 1);
      await page.evaluate(() => window.__sources.updateLeftSource({ query: 'SELECT changed' }));
      await page.evaluate(success => window.__ipc.pending.shift().resolve(success
        ? { success: true, result: window.__result('stale') }
        : { success: false, error: 'stale failure' }), success);
      await page.waitForFunction(() => !window.__sources.leftSource.loading);
      // Let the old request's completion publish if it is still incorrectly accepted.
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const state = await page.evaluate(() => window.__sources.leftSource);
      assert.equal(state.query, 'SELECT changed');
      assert.equal(state.result, undefined);
      assert.equal(state.error, undefined);
    });
  }

  await check('refresh compares the new responses', async () => {
    await page.evaluate(() => window.__sources.executeBoth());
    await page.waitForFunction(() => window.__sources.leftSource.result && window.__sources.rightSource.result);
    await page.evaluate(() => { window.__sources.setKeyColumns(['id']); });
    await page.evaluate(() => window.__sources.compare());
    await page.waitForFunction(() => window.__sources.diffResult?.stats.unchanged === 1);
    await page.evaluate(() => { window.__ipc.value = 'changed'; return window.__sources.refresh(); });
    await page.waitForFunction(() => window.__sources.diffResult?.stats.modified === 1, null, { timeout: 2000 });
  });

  await check('concurrent sides share one session and closing disconnects it', async () => {
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'connect_saved_connection').length), 1);
    assert.equal(await page.evaluate(() => window.__sources.leftSource.sessionId === window.__sources.rightSource.sessionId), true);
    await page.evaluate(() => window.__unmount());
    await page.waitForFunction(() => window.__ipc.calls.some(c => c.command === 'disconnect'), null, { timeout: 2000 });
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'disconnect').length), 1);
  });

  await check('changing the row limit reads with the selected value immediately', async () => {
    await page.evaluate(() => window.__sources.updateLeftSource({ mode: 'table', tableName: 'items' }));
    await page.waitForFunction(() => window.__sources.leftSource.result);
    await page.evaluate(() => { window.__ipc.calls = []; window.__setLimitAndRefresh(5000); });
    await page.waitForFunction(() => window.__ipc.calls.some(c => c.command === 'preview_table'));
    assert.equal(await page.evaluate(() => window.__ipc.calls.find(c => c.command === 'preview_table').args.limit), 5000);
  });

  await check('late snapshot results cannot replace a changed source', async () => {
    await page.evaluate(() => { window.__ipc.hold = true; window.__sources.updateLeftSource({ mode: 'snapshot', snapshotId: 'old' }); });
    await page.waitForFunction(() => window.__ipc.pending.length > 0);
    await page.evaluate(() => window.__sources.updateLeftSource({ mode: 'query', query: 'SELECT changed' }));
    await page.evaluate(() => window.__ipc.pending.shift().resolve({ success: true, result: window.__result('old snapshot') }));
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await page.evaluate(() => window.__sources.leftSource.result), undefined);
  });

  await check('a failed snapshot stops fetching and exposes its error', async () => {
    await page.evaluate(() => window.__sources.updateLeftSource({ mode: 'snapshot', snapshotId: 'missing' }));
    await page.waitForFunction(() => window.__sources.leftSource.error === 'synthetic missing snapshot');
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'get_snapshot').length), 1);
    assert.equal(await page.evaluate(() => window.__sources.leftSource.loading), false);
  });

  await check('query truncation is propagated to the comparison', async () => {
    await page.evaluate(() => { window.__ipc.truncated = true; return window.__sources.executeBoth(); });
    await page.waitForFunction(() => window.__sources.leftSource.result && window.__sources.rightSource.result);
    await page.evaluate(() => window.__sources.compare());
    await page.waitForFunction(() => window.__sources.diffResult?.warnings.includes('truncated'));
    assert.equal(await page.evaluate(() => window.__sources.diffResult.incomplete), true);
  });

  await check('a failed refresh clears the previous comparison', async () => {
    await page.evaluate(() => window.__sources.executeBoth());
    await page.waitForFunction(() => window.__sources.leftSource.result && window.__sources.rightSource.result);
    await page.evaluate(() => window.__sources.compare());
    await page.waitForFunction(() => window.__sources.diffResult);
    await page.evaluate(() => { window.__ipc.rejectLeft = true; return window.__sources.refresh(); });
    await page.waitForFunction(() => window.__sources.leftSource.error === 'synthetic query failure');
    assert.equal(await page.evaluate(() => window.__sources.diffResult), null);
    assert.equal(await page.evaluate(() => window.__sources.canCompare), false);
  });

  await check('changing the key invalidates the previous comparison', async () => {
    await page.evaluate(() => window.__sources.executeBoth());
    await page.waitForFunction(() => window.__sources.leftSource.result && window.__sources.rightSource.result);
    await page.evaluate(() => window.__sources.compare());
    await page.waitForFunction(() => window.__sources.diffResult);
    await page.evaluate(() => window.__sources.setKeyColumns(['id']));
    await page.waitForFunction(() => window.__sources.diffResult === null);
  });

  await check('failed namespace loading releases its session and allows an explicit retry', async () => {
    await page.evaluate(() => { window.__ipc.failNamespaces = true; return window.__sources.setLeftConnection({ id: 'other', driver: 'sqlite' }); });
    await page.waitForFunction(() => window.__sources.leftSource.connectionError === 'synthetic namespace failure');
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'disconnect' && c.args.sessionId === 'session-2').length), 1);
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'connect_saved_connection').length), 2);
    await page.evaluate(() => { window.__ipc.failNamespaces = false; return window.__sources.setLeftConnection({ id: 'other', driver: 'sqlite' }); });
    await page.waitForFunction(() => window.__sources.leftSource.sessionId === 'session-3');
  });

  await check('closing during connection establishment releases the late session exactly once', async () => {
    await page.evaluate(() => { window.__ipc.holdConnect = true; void window.__sources.setLeftConnection({ id: 'delayed', driver: 'sqlite' }); });
    await page.waitForFunction(() => window.__ipc.pending.length === 1);
    await page.evaluate(() => window.__unmount());
    await page.evaluate(() => window.__ipc.pending.shift().resolve({ success: true, session_id: 'late-session' }));
    await page.waitForFunction(() => window.__ipc.calls.some(c => c.command === 'disconnect' && c.args.sessionId === 'late-session'));
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'disconnect' && c.args.sessionId === 'late-session').length), 1);
  });

  await check('workspace changes clear sources, reject pending results and close owned sessions', async () => {
    await page.evaluate(() => { window.__ipc.hold = true; void window.__sources.executeLeft(); });
    await page.waitForFunction(() => window.__ipc.pending.length === 1);
    await page.evaluate(() => { window.__workspace = 'other-workspace'; window.__sources.setRowLimit(5000); });
    await page.waitForFunction(() => !window.__sources.leftSource.sessionId && !window.__sources.rightSource.sessionId);
    await page.evaluate(() => window.__ipc.pending.shift().resolve({ success: true, result: window.__result('stale workspace') }));
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await page.evaluate(() => window.__sources.leftSource.result), undefined);
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'disconnect').length), 1);
  });
  assert.deepEqual(errors, []);
  assert.deepEqual(failures, []);
} finally {
  await browser.close();
}
