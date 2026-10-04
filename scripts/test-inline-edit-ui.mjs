// SPDX-License-Identifier: Apache-2.0

import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { dirname } from 'node:path';
const { chromium } = await import(process.env.QOREDB_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.QOREDB_CHROMIUM_EXECUTABLE,
});
const page = await browser.newPage({ viewport: { width: 1200, height: 750 } });
const errors = [];
const requests = [];
page.on('request', request => requests.push(request.url()));
page.on('pageerror', error => errors.push(error.message));
const providers = {
  SessionProvider:
    'export const useSessionContext = () => ({savedConnections:[],refreshSidebar(){}});',
  WorkspaceProvider:
    'export const useWorkspace = () => ({projectId:"fixture",activeWorkspace:null});',
  LicenseProvider:
    'export const useLicense = () => ({isFeatureEnabled:feature=>feature==="data_generator",tier:"core",status:{tier:"core"},loading:false});',
  AiPreferencesProvider:
    'export const useAiPreferences = () => ({getConfig:()=>null,isReady:false});',
  PluginProvider:
    'export const usePlugins = () => ({contributions:{resultViewers:[],commands:[],themes:[]}});',
};
for (const [name, body] of Object.entries(providers)) {
  await page.route(`**/src/providers/${name}.tsx*`, route =>
    route.fulfill({ contentType: 'application/javascript', body })
  );
}
await page.addInitScript(() => {
  window.$RefreshReg$ = () => {};
  window.$RefreshSig$ = () => type => type;
  const columns = [
    { name: 'id', data_type: 'integer', nullable: false },
    { name: 'name', data_type: 'text', nullable: false },
    { name: 'rank', data_type: 'integer', nullable: false },
    { name: 'bytes', data_type: 'BLOB', nullable: true },
  ];
  const rows = Array.from({ length: 1000 }, (_, i) => ({
    values: [i, `record-${i}`, i % 3, 'aGVsbG8='],
  }));
  window.__db = {
    rows,
    calls: [],
    delay: 50,
    completed: 0,
    failRead: false,
    failWrite: false,
    changeRank: false,
    changeKey: false,
  };
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 1,
    invoke: async (command, args) => {
      window.__db.calls.push({ command, args });
      if (command === 'plugin:event|listen') return 1;
      if (command === 'plugin:event|unlisten') return;
      if (command === 'update_row') {
        await new Promise(resolve => setTimeout(resolve, window.__db.delay));
        window.__db.completed++;
        if (window.__db.failWrite) return { success: false, error: 'test refusal' };
        const row = rows.find(row => row.values[0] === args.primaryKey.columns.id);
        for (const [name, value] of Object.entries(args.data.columns))
          row.values[columns.findIndex(c => c.name === name)] =
            typeof value === 'string' ? value.trim().toUpperCase() : value;
        if (window.__db.changeRank) row.values[2] += 1;
        if (window.__db.changeKey) row.values[0] += 10000;
        return {
          success: true,
          result: { columns: [], rows: [], affected_rows: 1, execution_time_ms: 1 },
        };
      }
      if (command === 'query_table') {
        const options = args.options;
        if (options.filters?.length && window.__db.failRead) throw new Error('test read failure');
        let matching = rows;
        for (const filter of options.filters ?? [])
          matching = matching.filter(
            row => row.values[columns.findIndex(c => c.name === filter.column)] === filter.value
          );
        const offset = options.cursor
          ? Number(options.cursor)
          : (options.page - 1) * options.page_size;
        const resultRows = structuredClone(matching.slice(offset, offset + options.page_size));
        return {
          success: true,
          result: {
            result: { columns, rows: resultRows, execution_time_ms: 1 },
            page: options.page,
            page_size: options.page_size,
            total_rows: options.count_mode === 'estimated' ? matching.length : null,
            total_rows_source: options.count_mode === 'estimated' ? 'estimated' : null,
            total_rows_as_of: null,
            has_more: offset + options.page_size < matching.length,
            next_cursor: String(offset + options.page_size),
            pagination_strategy: 'keyset',
            ordering_guarantee: 'stable',
          },
        };
      }
      throw new Error('Unexpected fixture IPC: ' + command);
    },
  };
});
try {
  await page.goto(
    new URL(
      '/scripts/fixtures/inline-edit/index.html?lang=en',
      process.env.QOREDB_UI_BASE_URL ?? 'http://127.0.0.1:1430'
    ).href
  );
  await page.waitForFunction(() => window.__fixture?.data.loadedRows === 100);
  for (let count = 200; count <= 500; count += 100) {
    await page.locator('#more').click();
    await page.waitForFunction(n => window.__fixture.data.loadedRows === n, count);
  }
  const scroller = page.locator('[data-datagrid] > div').filter({ has: page.locator('table') });
  await scroller.evaluate(el => (el.scrollTop = 7000));
  const row = page
    .locator('tr')
    .filter({
      has: page
        .getByRole('button', { name: 'Edit cell: id', exact: true })
        .filter({ hasText: /^234$/ }),
    });
  await row.waitFor();
  await row.getByRole('checkbox').click();
  const cell = row.getByRole('button', { name: 'Edit cell: name', exact: true });
  const before = await scroller.evaluate(el => el.scrollTop);
  await cell.focus();
  await page.keyboard.press('Enter');
  const input = page.getByRole('textbox', { name: 'Edit cell', exact: true });
  await input.fill('  changed by test  ');
  await input.press('Enter');
  await page.waitForFunction(
    () => window.__fixture.data.data.rows[234].values[1] === 'CHANGED BY TEST'
  );
  assert.equal(await cell.textContent(), 'CHANGED BY TEST');
  assert.equal(await scroller.evaluate(el => el.scrollTop), before);
  assert.equal(await row.getByRole('checkbox').getAttribute('data-state'), 'checked');
  assert.equal(await cell.evaluate(el => el === document.activeElement), true);
  assert.equal(await page.locator('#loaded').textContent(), '500');
  assert.equal(
    await page.evaluate(
      () => window.__db.calls.filter(call => call.command === 'update_row').length
    ),
    1
  );
  console.log(
    'PASS five pages, canonical value, scroll, selection, focus, Enter/blur single write'
  );
  await cell.press('F2');
  await input.fill('cancelled');
  await input.press('Escape');
  assert.equal(await cell.textContent(), 'CHANGED BY TEST');
  await page.waitForFunction(
    () => document.activeElement?.getAttribute('aria-label') === 'Edit cell: name'
  );
  assert.equal(
    await page.evaluate(
      () => window.__db.calls.filter(call => call.command === 'update_row').length
    ),
    1
  );
  console.log('PASS F2 and Escape restore focus without writing');
  await cell.press('Enter');
  await input.fill('blur saved');
  await page.locator('#outside').click();
  await page.waitForFunction(() => window.__fixture.data.data.rows[234].values[1] === 'BLUR SAVED');
  assert.equal(await page.locator('#outside').evaluate(el => el === document.activeElement), true);
  console.log('PASS blur saves without stealing focus');
  await page.evaluate(() => (window.__db.failWrite = true));
  const completed = await page.evaluate(() => window.__db.completed);
  await cell.press('Enter');
  await input.fill('refused');
  await input.press('Enter');
  await page.waitForFunction(n => window.__db.completed === n + 1, completed);
  assert.equal(await cell.textContent(), 'BLUR SAVED');
  assert.equal(await page.locator('#loaded').textContent(), '500');
  console.log('PASS server refusal keeps the current pages and saved value');
  await page.evaluate(() => {
    window.__db.failWrite = false;
    window.__db.delay = 150;
  });
  await cell.press('Enter');
  await input.fill('concurrent page');
  await input.press('Enter');
  await page.evaluate(() => window.__fixture.data.fetchNextChunk());
  await page.waitForFunction(
    () =>
      window.__fixture.data.loadedRows === 600 &&
      window.__fixture.data.data.rows[234].values[1] === 'CONCURRENT PAGE'
  );
  assert.equal(await cell.textContent(), 'CONCURRENT PAGE');
  assert.equal(await row.getByRole('checkbox').getAttribute('data-state'), 'checked');
  assert.equal(await scroller.evaluate(el => el.scrollTop), before);
  console.log('PASS concurrent sixth page is retained with the confirmed edit');
  await page.evaluate(() => (window.__db.failRead = true));
  const callsBefore = await page.evaluate(
    () => window.__db.calls.filter(call => call.command === 'update_row').length
  );
  await cell.press('Enter');
  await input.fill('read failed');
  await input.press('Enter');
  await page.waitForFunction(() => window.__fixture.data.loadedRows === 100);
  assert.equal(
    await page.evaluate(
      () => window.__db.calls.filter(call => call.command === 'update_row').length
    ),
    callsBefore + 1
  );
  assert.equal(await page.evaluate(() => window.__db.rows[234].values[1]), 'READ FAILED');
  assert.equal(await scroller.evaluate(el => el.scrollTop), 0);
  console.log('PASS canonical read failure reloads without repeating the write');
  await page.evaluate(() => {
    window.__db.failRead = false;
    window.__db.delay = 300;
  });
  const first = page
    .locator('tr')
    .filter({
      has: page
        .getByRole('button', { name: 'Edit cell: id', exact: true })
        .filter({ hasText: /^0$/ }),
    });
  const firstCell = first.getByRole('button', { name: 'Edit cell: name', exact: true });
  const completedBefore = await page.evaluate(() => window.__db.completed);
  await firstCell.press('Enter');
  await input.fill('old session only');
  await input.press('Enter');
  await page.waitForFunction(
    n => window.__db.calls.filter(c => c.command === 'update_row').length === n + 2,
    callsBefore
  );
  await page.evaluate(() => window.__fixture.setSession('fixture-b'));
  await page.waitForFunction(
    () =>
      window.__fixture.data.loadedRows === 100 &&
      window.__db.calls.some(c => c.command === 'query_table' && c.args.sessionId === 'fixture-b')
  );
  await page.waitForFunction(n => window.__db.completed === n + 1, completedBefore);
  assert.equal(await page.evaluate(() => window.__fixture.data.data.rows[0].values[1]), 'record-0');
  assert.equal(
    await page.evaluate(
      () =>
        window.__db.calls.filter(
          c =>
            c.command === 'query_table' &&
            c.args.sessionId === 'fixture-b' &&
            c.args.options.filters?.length
        ).length
    ),
    0
  );
  assert.equal(await page.getByText('1 row(s) selected', { exact: true }).count(), 0);
  console.log('PASS late mutation and selection do not leak into another connection view');
  assert.equal(
    requests.some(url => url.includes('/Grid/BlobViewer.tsx')),
    false
  );
  await first.getByRole('button', { name: 'Binary Data', exact: true }).click();
  await page.getByRole('dialog').waitFor();
  assert.equal(
    requests.some(url => url.includes('/Grid/BlobViewer.tsx')),
    true
  );
  await page.keyboard.press('Escape');
  await page.getByRole('dialog').waitFor({ state: 'hidden' });
  await first.getByRole('button', { name: 'Binary Data', exact: true }).click();
  await page.getByRole('dialog').waitFor();
  await page.keyboard.press('Escape');
  await page.getByRole('dialog').waitFor({ state: 'hidden' });
  console.log('PASS binary viewer loads on first opening and reopens correctly');
  assert.equal(
    requests.some(url => url.includes('/Grid/DataGeneratorDialog.tsx')),
    false
  );
  await page.getByTitle('Generate data').click();
  await page.getByRole('dialog').waitFor();
  assert.equal(
    requests.some(url => url.includes('/Grid/DataGeneratorDialog.tsx')),
    true
  );
  await page.locator('#dg-count').fill('123');
  await page.keyboard.press('Escape');
  await page.getByRole('dialog').waitFor({ state: 'hidden' });
  await page.getByTitle('Generate data').click();
  await page.getByRole('dialog').waitFor();
  assert.equal(await page.locator('#dg-count').inputValue(), '123');
  await page.keyboard.press('Escape');
  await page.getByRole('dialog').waitFor({ state: 'hidden' });
  console.log('PASS generator loads on demand and keeps form state after reopening');
  assert.equal(
    requests.some(url => url.includes('/Snapshot/SaveSnapshotDialog.tsx')),
    false
  );
  await page.getByRole('button', { name: 'Export', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Save as snapshot', exact: true }).click();
  await page.getByRole('dialog').waitFor();
  assert.equal(
    requests.some(url => url.includes('/Snapshot/SaveSnapshotDialog.tsx')),
    true
  );
  await page.locator('#snapshot-name').fill('Kept draft');
  await page.keyboard.press('Escape');
  await page.getByRole('dialog').waitFor({ state: 'hidden' });
  await page.getByRole('button', { name: 'Export', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Save as snapshot', exact: true }).click();
  await page.getByRole('dialog').waitFor();
  assert.equal(await page.locator('#snapshot-name').inputValue(), 'Kept draft');
  await page.keyboard.press('Escape');
  await page.getByRole('dialog').waitFor({ state: 'hidden' });
  await page.locator('[data-slot="dialog-overlay"]').waitFor({ state: 'detached' });
  console.log('PASS snapshot loads on demand and keeps its draft after reopening');
  const screenshot = process.env.QOREDB_UI_SCREENSHOT ?? '.perf/inline-edit-ui.png';
  await mkdir(dirname(screenshot), { recursive: true });
  await page.screenshot({ path: screenshot });
  assert.deepEqual(errors, []);
} catch (error) {
  console.log('UI errors:', errors);
  console.log((await page.locator('body').innerText()).slice(0, 1800));
  throw error;
} finally {
  await browser.close();
}
