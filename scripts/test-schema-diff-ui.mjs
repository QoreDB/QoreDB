// SPDX-License-Identifier: BUSL-1.1

import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';

const { chromium } = await import(process.env.QOREDB_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({ headless: true, executablePath: process.env.QOREDB_CHROMIUM_EXECUTABLE });
const page = await browser.newPage({ viewport: { width: 700, height: 650 } });
const errors = [];
page.on('pageerror', error => errors.push(error.message));
await page.route('**/src/providers/WorkspaceProvider.tsx*', route => route.fulfill({
  contentType: 'application/javascript', body: 'export const useWorkspace = () => ({projectId:"synthetic"});',
}));
await page.addInitScript(() => {
  window.$RefreshReg$ = () => {};
  window.$RefreshSig$ = () => type => type;
  const scenario = new URLSearchParams(location.search).get('case');
  window.__ipc = { calls: [], pending: [] };
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 1,
    invoke: async (command, args) => {
      window.__ipc.calls.push({ command, args });
      if (command === 'list_saved_connections') return ['left', 'right'].map(id => ({ id, name: id, driver: 'sqlite', database: 'fixture' }));
      if (command === 'connect_saved_connection') {
        if (scenario === 'late') return new Promise(resolve => window.__ipc.pending.push(resolve));
        return { success: true, session_id: args.connectionId };
      }
      if (command === 'disconnect') return { success: true };
      if (command === 'list_namespaces') return scenario === 'namespaces-error'
        ? { success: false, error: 'synthetic namespace failure' }
        : { success: true, namespaces: [{ database: 'fixture' }] };
      if (command === 'list_collections') {
        if (scenario === 'tables-error') return { success: false, error: 'synthetic table-list failure' };
        if (scenario === 'describe-error') return { success: true, data: { collections: [{ namespace: { database: 'fixture' }, name: 'users', collection_type: 'Table' }], total_count: 1 } };
        if (scenario === 'missing-payload') return { success: true };
        return { success: true, data: { collections: [], total_count: 0 } };
      }
      if (command === 'describe_table') return { success: false, error: 'synthetic describe failure' };
      throw new Error('Unexpected schema fixture IPC: ' + command);
    },
  };
});
const base = process.env.QOREDB_UI_BASE_URL ?? 'http://127.0.0.1:1430';
async function fresh(scenario, language = 'en') {
  await page.goto(new URL(`/scripts/fixtures/schema-diff/index.html?case=${scenario}&lang=${language}`, base).href);
}
try {
  await fresh('empty');
  await page.getByText('Schemas are identical.', { exact: true }).waitFor();
  console.log('PASS valid empty schemas are identical');
  for (const [scenario, message] of [['namespaces-error', 'synthetic namespace failure'], ['tables-error', 'synthetic table-list failure']]) {
    await fresh(scenario);
    await page.getByText('Could not connect to compare: ' + message, { exact: true }).waitFor();
    assert.equal(await page.getByText('Schemas are identical.', { exact: true }).count(), 0);
    await page.evaluate(() => window.__closeSchema());
    await page.waitForFunction(() => window.__ipc.calls.filter(c => c.command === 'disconnect').length === 2);
    console.log('PASS ' + scenario + ' is an error, never equality, and closing releases sessions');
  }
  await fresh('missing-payload', 'fr');
  await page.getByText(/Capture du schéma incomplète/).waitFor();
  await mkdir('.perf', { recursive: true });
  await page.screenshot({ path: '.perf/schema-diff-incomplete.png' });
  console.log('PASS malformed response has a translated incomplete-capture error');
  await fresh('describe-error');
  await page.getByText('Some tables could not be described; the diff may be incomplete.', { exact: true }).waitFor();
  assert.equal(await page.getByText('Schemas are identical.', { exact: true }).count(), 0);
  console.log('PASS incomplete descriptions never claim schema equality');
  await fresh('late');
  await page.waitForFunction(() => window.__ipc.pending.length === 1);
  await page.evaluate(() => window.__closeSchema());
  await page.evaluate(() => window.__ipc.pending.shift()({ success: true, session_id: 'late-session' }));
  await page.waitForFunction(() => window.__ipc.calls.some(c => c.command === 'disconnect' && c.args.sessionId === 'late-session'), null, { timeout: 3000 });
  assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'connect_saved_connection').length), 1);
  assert.equal(await page.evaluate(() => window.__ipc.calls.filter(c => c.command === 'list_namespaces').length), 0);
  assert.deepEqual(errors, []);
  console.log('PASS late connection is released without opening the second source');
  console.log('6 schema-diff scenarios passed (simulated IPC).');
} finally {
  await browser.close();
}
