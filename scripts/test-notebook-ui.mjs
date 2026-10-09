// SPDX-License-Identifier: BUSL-1.1

import assert from 'node:assert/strict';

const { chromium } = await import(process.env.QOREDB_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({ headless: true, executablePath: process.env.QOREDB_CHROMIUM_EXECUTABLE });
const page = await browser.newPage();
page.setDefaultTimeout(3000);
const errors = [];
const failures = [];
let passed = 0;
page.on('pageerror', error => { errors.push(error.message); console.log('PAGEERROR ' + error.message); });
await page.route('**/node_modules/.vite/deps/sonner.js*', route => route.fulfill({
  contentType: 'application/javascript',
  body: 'export const toast = Object.fromEntries(["success","error","info"].map(type => [type, message => window.__toasts.push({type,message})]));',
}));
await page.route('**/node_modules/.vite/deps/@tauri-apps_plugin-fs.js*', route => route.fulfill({
  contentType: 'application/javascript',
  body: `export async function readTextFile(path) {
    const value = window.__files[path];
    if (window.__fileState.holdRead) await new Promise(resolve => window.__fileState.pendingRead = resolve);
    if (value === undefined) throw new Error('Synthetic missing file');
    return value;
  }
  export async function writeTextFile(path, content) {
    if (window.__fileState.holdWrite) await new Promise(resolve => window.__fileState.pendingWrite = resolve);
    if (window.__fileState.failWrite) throw new Error('Synthetic disk failure');
    window.__files[path] = content;
  }`,
}));
for (const [path, name] of [
  ['Connection/ConnectionModal', 'ConnectionModal'], ['Crash/CrashReportOverlay', 'CrashReportOverlay'],
  ['License/ProActivationDialog', 'ProActivationDialog'], ['Newsletter/NewsletterPromptModal', 'NewsletterPromptModal'],
  ['Onboarding/OnboardingModal', 'OnboardingModal'], ['Search/FulltextSearchPanel', 'FulltextSearchPanel'],
  ['Search/GlobalSearch', 'GlobalSearch'], ['WhatsNew/WhatsNewModal', 'WhatsNewModal'],
]) {
  await page.route(`**/src/components/${path}.tsx*`, route => route.fulfill({
    contentType: 'application/javascript', body: `export const ${name} = () => null;`,
  }));
}
await page.route('**/src/hooks/useWhatsNew.ts*', route => route.fulfill({
  contentType: 'application/javascript', body: 'export const useWhatsNew = () => {}; export const getChangelogFor = () => []; export const markVersionSeen = () => {};',
}));
await page.route('**/src/providers/LicenseProvider.tsx*', route => route.fulfill({
  contentType: 'application/javascript', body: 'export const useLicense = () => ({isFeatureEnabled:()=>true});',
}));
const libraryRequests = [];
page.on('request', request => {
  if (request.url().includes('/QueryLibraryModal.tsx')) libraryRequests.push(request.url());
});
await page.addInitScript(() => {
  window.$RefreshReg$ = () => {};
  window.$RefreshSig$ = () => type => type;
  window.__toasts = [];
  window.__files = {};
  window.__fileState = { savePath: "/synthetic.qnb", openPath: null };
  window.__ipc = { calls: [], pending: [], hold: false, value: 1 };
  window.__result = value => ({ columns: [{ name: 'id', data_type: 'integer', nullable: false }], rows: [{ values: [value] }], execution_time_ms: 0 });
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 1,
    invoke: async (command, args) => {
      window.__ipc.calls.push({ command, args });
      if (command === 'plugin:dialog|save') return window.__fileState.savePath;
      if (command === 'plugin:dialog|open') return window.__fileState.openPath;
      if (command === 'cancel_query') return { success: true };
      if (command === 'execute_query') {
        if (window.__ipc.hold) return new Promise(resolve => window.__ipc.pending.push({ resolve }));
        if (window.__ipc.fail) return { success: false, error: 'synthetic failure' };
        const result = window.__result(window.__ipc.value);
        if (window.__ipc.masked) result.columns[0].masked = true;
        return { success: true, truncated: window.__ipc.truncated, result };
      }
      throw new Error('Unexpected fixture IPC: ' + command);
    },
  };
});
const url = new URL('/scripts/fixtures/notebook/index.html?lang=en', process.env.QOREDB_UI_BASE_URL ?? 'http://127.0.0.1:1430').href;
async function settle() {
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}
async function check(name, run) {
  try {
    await page.goto(url);
    await page.waitForFunction(() => window.__notebook);
    await run();
    passed++;
    console.log('PASS ' + name);
  } catch (error) {
    failures.push(name + ': ' + error.message);
    console.log('FAIL ' + name + ': ' + error.message);
  }
}
async function runAll() {
  await page.evaluate(() => window.__notebook.executeAll());
  await settle();
}
async function holdCell() {
  await page.evaluate(() => { window.__ipc.hold = true; void window.__notebook.executeCell('a'); });
  await page.waitForFunction(() => window.__ipc.pending.length === 1);
}
async function releaseCell() {
  await page.evaluate(() => window.__ipc.pending.shift().resolve({ success: true, result: window.__result(99) }));
  await settle();
}
try {
  await check('batch uses freshly returned results without waiting for a render', async () => {
    await runAll();
    assert.deepEqual(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'execute_query').map(c => c.args.query)), ['SELECT 1', 'SELECT 1', 'SELECT 1']);
  });
  for (const edit of ['source', 'variable', 'removeVariable', 'replaceVariable', 'delete']) {
    await check(edit + ' invalidates transitive references and blocks dependent SQL', async () => {
      await runAll();
      await page.evaluate(edit => {
        const nb = window.__notebook;
        if (edit === 'source') nb.updateCellSource('a', 'SELECT 2');
        if (edit === 'variable') nb.updateVariable('value', '2');
        if (edit === 'removeVariable') nb.removeVariable('value');
        if (edit === 'replaceVariable') nb.addVariable({ name: 'value', type: 'number', defaultValue: '2' });
        if (edit === 'delete') nb.deleteCell('a');
        window.__ipc.calls = [];
      }, edit);
      await settle();
      assert.equal(await page.evaluate(() => window.__notebook.notebook.cells.find(c => c.id === 'b').executionState), 'stale');
      assert.equal(await page.evaluate(() => window.__notebook.notebook.cells.find(c => c.id === 'c').executionState), 'stale');
      await page.evaluate(() => window.__notebook.executeCell('c'));
      assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'execute_query').length), 0);
      assert.equal(await page.evaluate(() => window.__notebook.notebook.cells.find(c => c.id === 'c').lastResult.type), 'error');
    });
  }
  for (const change of ['cancel', 'edit', 'clear', 'session', 'namespace', 'workspace']) {
    await check('late response is discarded after ' + change, async () => {
      await holdCell();
      await page.evaluate(change => {
        const nb = window.__notebook;
        if (change === 'cancel') nb.cancelExecution();
        if (change === 'edit') nb.updateCellSource('a', 'SELECT 2');
        if (change === 'clear') nb.clearAllResults();
        if (change === 'session') window.__context({ sessionId: 'session-b' });
        if (change === 'namespace') window.__context({ sessionId: 'session-a', namespace: { database: 'other' } });
        if (change === 'workspace') window.__workspace('other');
      }, change);
      await settle();
      await releaseCell();
      assert.notEqual(await page.evaluate(() => window.__notebook.notebook.cells[0].executionState), 'success');
      assert.equal(await page.evaluate(() => window.__notebook.notebook.cells[0].lastResult?.rows), undefined);
      assert.equal(await page.evaluate(() => window.__notebook.isExecuting), false);
    });
  }
  await check('failed batch never reports all cells executed', async () => {
    await page.evaluate(() => { window.__ipc.fail = true; });
    await runAll();
    assert.equal(await page.evaluate(() => window.__toasts.filter(t => t.type === 'success').length), 0);
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'execute_query').length), 1);
  });
  await check('two same-tick execution requests dispatch only once', async () => {
    await page.evaluate(() => { window.__ipc.hold = true; void window.__notebook.executeCell('a'); void window.__notebook.executeCell('a'); });
    await settle();
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'execute_query').length), 1);
    await releaseCell();
  });
  await check('closing the notebook cancels its pending query', async () => {
    await holdCell();
    await page.evaluate(() => window.__unmount());
    await settle();
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'cancel_query').length), 1);
    await releaseCell();
  });
  await check('undo and redo change execution input without reviving results', async () => {
    await page.evaluate(() => window.__notebook.updateCellSource('a', 'SELECT 7'));
    await settle();
    await page.evaluate(() => window.__notebook.undo());
    await settle();
    assert.equal(await page.evaluate(() => window.__notebook.notebook.cells[0].source), 'SELECT $value');
    await page.evaluate(() => window.__notebook.redo());
    await settle();
    assert.equal(await page.evaluate(() => window.__notebook.notebook.cells[0].source), 'SELECT 7');
    await page.evaluate(() => window.__notebook.executeCell('a'));
    assert.equal(await page.evaluate(() => window.__ipc.calls.find(c => c.command === 'execute_query').args.query), 'SELECT 7');
  });
  await check('continue on error executes independent cells without claiming full success', async () => {
    await page.evaluate(() => { window.__ipc.fail = true; return window.__notebook.executeAll(true); });
    await settle();
    assert.equal(await page.evaluate(() => window.__toasts.filter(t => t.type === 'success').length), 0);
    assert.equal(await page.evaluate(() => window.__notebook.notebook.cells.every(c => c.executionState === 'error')), true);
    assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'execute_query').length), 1);
  });
  await check('retry recomputes a stale dependency chain', async () => {
    await runAll();
    await page.evaluate(() => { window.__notebook.updateVariable('value', '42'); window.__ipc.value = 42; window.__ipc.calls = []; });
    await runAll();
    assert.deepEqual(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'execute_query').map(c => c.args.query)), ['SELECT 42', 'SELECT 42', 'SELECT 42']);
    assert.equal(await page.evaluate(() => window.__notebook.notebook.cells.every(c => c.executionState === 'success')), true);
  });
  await check('edits made during save stay dirty and remain in the draft', async () => {
    await page.evaluate(() => { window.__notebook.updateCellSource('a', 'SELECT 7'); window.__fileState.holdWrite = true; void window.__notebook.save(); });
    await page.waitForFunction(() => window.__fileState.pendingWrite);
    await page.evaluate(() => { window.__notebook.updateCellSource('a', 'SELECT 8'); window.__fileState.pendingWrite(); });
    await settle();
    assert.equal(await page.evaluate(() => window.__notebook.isDirty), true);
    assert.equal(await page.evaluate(() => JSON.parse(window.__files['/synthetic.qnb']).cells[0].source), 'SELECT 7');
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('qnb_draft_fixture')).cells[0].source), 'SELECT 8');
  });
  await check('reopening a saved notebook replaces runtime input and history', async () => {
    await page.evaluate(() => { window.__notebook.setTitle('Saved revision'); return window.__notebook.save(); });
    await settle();
    await page.evaluate(() => { window.__fileState.openPath = '/synthetic.qnb'; const data = JSON.parse(window.__files['/synthetic.qnb']); data.cells[0].source = 'SELECT 123'; window.__files['/synthetic.qnb'] = JSON.stringify(data); return window.__notebook.openFromFile(); });
    await settle();
    assert.equal(await page.evaluate(() => window.__notebook.notebook.cells[0].source), 'SELECT 123');
    assert.equal(await page.evaluate(() => window.__notebook.canUndo), false);
    await page.evaluate(() => window.__notebook.executeCell('a'));
    assert.equal(await page.evaluate(() => window.__ipc.calls.find(c => c.command === 'execute_query').args.query), 'SELECT 123');
  });
  await check('import asks before discarding unsaved work and respects cancellation', async () => {
    await page.evaluate(() => { window.__notebook.updateCellSource('a', 'SELECT 7'); window.__files['/import.sql'] = 'SELECT 123'; window.__fileState.openPath = '/import.sql'; void window.__notebook.importFromFile(); });
    await page.waitForFunction(() => window.__confirm.open);
    await page.evaluate(() => window.__resolveConfirm(false));
    await settle();
    assert.equal(await page.evaluate(() => window.__notebook.notebook.cells[0].source), 'SELECT 7');
    assert.equal(await page.evaluate(() => window.__notebook.isDirty), true);
  });
  await check('imported cells execute their own source and are immediately recoverable', async () => {
    await page.evaluate(() => { window.__files['/import.sql'] = 'SELECT 123'; window.__fileState.openPath = '/import.sql'; return window.__notebook.importFromFile(); });
    await settle();
    assert.equal(await page.evaluate(() => window.__notebook.canUndo), false);
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('qnb_draft_fixture')).cells[0].source), 'SELECT 123');
    await page.evaluate(() => window.__notebook.executeAll());
    assert.equal(await page.evaluate(() => window.__ipc.calls.find(c => c.command === 'execute_query').args.query), 'SELECT 123');
  });
  await check('edits made while a file is read are not silently replaced', async () => {
    await page.evaluate(() => { window.__files['/import.sql'] = 'SELECT 123'; window.__fileState.openPath = '/import.sql'; window.__fileState.holdRead = true; void window.__notebook.importFromFile(); });
    await page.waitForFunction(() => window.__fileState.pendingRead);
    await page.evaluate(() => { window.__notebook.updateCellSource('a', 'SELECT 7'); window.__fileState.pendingRead(); });
    await settle();
    assert.equal(await page.evaluate(() => window.__notebook.notebook.cells[0].source), 'SELECT 7');
  });
  await check('cancelled or failed saves preserve the current dirty notebook', async () => {
    await page.evaluate(() => { window.__notebook.setTitle('Changed'); window.__fileState.savePath = null; return window.__notebook.save(); });
    assert.equal(await page.evaluate(() => window.__notebook.isDirty), true);
    await page.evaluate(() => { window.__fileState.savePath = '/synthetic.qnb'; window.__fileState.failWrite = true; return window.__notebook.save(); });
    assert.equal(await page.evaluate(() => window.__notebook.isDirty), true);
    assert.equal(await page.evaluate(() => window.__files['/synthetic.qnb']), undefined);
  });
  await check('a late save cannot clear dirty state after a workspace change', async () => {
    await page.evaluate(() => { window.__notebook.setTitle('Saved revision'); window.__fileState.holdWrite = true; void window.__notebook.save(); });
    await page.waitForFunction(() => window.__fileState.pendingWrite);
    await page.evaluate(() => window.__workspace('other'));
    await settle();
    await page.evaluate(() => { window.__notebook.setTitle('New workspace revision'); window.__fileState.pendingWrite(); });
    await settle();
    assert.equal(await page.evaluate(() => window.__notebook.isDirty), true);
    assert.equal(await page.evaluate(() => window.__notebook.path), null);
  });
  await check('opening malformed data leaves the current notebook intact', async () => {
    await page.evaluate(() => { window.__files['/invalid.qnb'] = '{"version":1,"cells":[]}'; window.__fileState.openPath = '/invalid.qnb'; return window.__notebook.openFromFile(); });
    assert.equal(await page.evaluate(() => window.__notebook.notebook.metadata.title), 'Synthetic notebook');
    assert.equal(await page.evaluate(() => window.__toasts.filter(t => t.type === 'error').length), 1);
  });
  await check('invalid chart configuration cannot replace the current notebook', async () => {
    await page.evaluate(() => {
      const data = structuredClone(window.__notebook.notebook);
      data.metadata.title = 'Invalid imported chart';
      data.cells[0].type = 'chart';
      data.cells[0].config = {chartConfig:{sourceLabel:'a',type:'bar',xColumn:'id',yColumns:'invalid'}};
      window.__files['/invalid-chart.qnb'] = JSON.stringify(data);
      window.__fileState.openPath = '/invalid-chart.qnb';
      return window.__notebook.openFromFile();
    });
    assert.equal(await page.evaluate(() => window.__notebook.notebook.metadata.title), 'Synthetic notebook');
    assert.equal(await page.evaluate(() => window.__notebook.notebook.cells[0].type), 'sql');
    assert.equal(await page.evaluate(() => window.__toasts.filter(t => t.type === 'error').length), 1);
  });
  for (const limited of ['masked', 'truncated']) {
    await check(limited + ' sources cannot dispatch a dependent mutation', async () => {
      await page.evaluate(limited => { window.__ipc[limited] = true; window.__notebook.updateCellSource('b', 'DELETE FROM users WHERE id = $a.id'); return window.__notebook.executeAll(); }, limited);
      await settle();
      assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'execute_query').length), 1);
      assert.equal(await page.evaluate(() => window.__notebook.notebook.cells[1].executionState), 'error');
      assert.equal(await page.evaluate(() => window.__toasts.some(t => t.type === 'success')), false);
    });
  }
  await check('a restored draft is dirty and protected from replacement', async () => {
    await page.evaluate(() => { const draft = structuredClone(window.__notebook.notebook); draft.cells[0].source = 'SELECT 99'; localStorage.setItem('qnb_draft_fixture', JSON.stringify(draft)); });
    await page.goto(url + '&draft=1');
    await page.waitForFunction(() => window.__notebook);
    assert.equal(await page.evaluate(() => window.__notebook.notebook.cells[0].source), 'SELECT 99');
    assert.equal(await page.evaluate(() => window.__notebook.isDirty), true);
  });
  await page.goto(new URL('/scripts/fixtures/notebook/overlays.html?lang=en', process.env.QOREDB_UI_BASE_URL ?? 'http://127.0.0.1:1430').href);
  await page.waitForFunction(() => window.__libraryOpen);
  await settle();
  assert.equal(libraryRequests.length, 0, 'Library must not load at startup');
  await page.evaluate(() => window.__libraryOpen(true));
  const search = page.getByPlaceholder('Search title or query...');
  await search.waitFor();
  assert.equal(libraryRequests.length, 1);
  await search.fill('Retained search');
  await page.evaluate(() => window.__libraryOpen(false));
  await settle();
  await page.evaluate(() => window.__libraryOpen(true));
  assert.equal(await search.inputValue(), 'Retained search');
  console.log('PASS real AppOverlays defers the library and retains its state after closing');
  passed++;
  assert.deepEqual(errors, [], 'Browser errors');
  assert.deepEqual(failures, [], 'Notebook failures');
  console.log(`${passed} notebook scenarios passed (simulated IPC).`);
} finally {
  await browser.close();
}
