// SPDX-License-Identifier: BUSL-1.1

import assert from 'node:assert/strict';
const { chromium } = await import(process.env.QOREDB_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.QOREDB_CHROMIUM_EXECUTABLE,
});
const page = await browser.newPage({ viewport: { width: 1000, height: 850 } });
const errors = [];
page.on('pageerror', error => errors.push(error.message));
await page.route('**/src/providers/LicenseProvider.tsx*', route =>
  route.fulfill({
    contentType: 'application/javascript',
    body: 'export const useLicense = () => ({isFeatureEnabled:()=>!location.search.includes("core=1")});',
  })
);
await page.route('**/src/providers/WorkspaceProvider.tsx*', route =>
  route.fulfill({
    contentType: 'application/javascript',
    body: 'export const useWorkspace = () => ({projectId:"fixture",activeWorkspace:null});',
  })
);
await page.addInitScript(() => {
  window.$RefreshReg$ = () => {};
  window.$RefreshSig$ = () => type => type;
  window.__settings = {
    calls: [],
    failRead: location.search.includes('failRead=1'),
    failWrite: false,
    rejectWrite: false,
    pendingRead: true,
    pendingWrite: true,
    config: {
      enabled: true,
      max_entries: 50000,
      retention_days: 30,
      max_file_size_mb: 500,
      excluded_tables: [],
      production_only: false,
      sensitive_columns: ['private_note'],
    },
  };
  window.__TAURI_INTERNALS__ = {
    invoke: async (command, args) => {
      const state = window.__settings;
      state.calls.push({ command, args: structuredClone(args) });
      if (command === 'get_time_travel_config') {
        while (state.pendingRead) await new Promise(resolve => setTimeout(resolve, 5));
        if (state.failRead) throw new Error('fixture read failure');
        return { success: true, config: structuredClone(state.config) };
      }
      if (command === 'update_time_travel_config') {
        while (state.pendingWrite) await new Promise(resolve => setTimeout(resolve, 5));
        if (state.failWrite) throw new Error('fixture persistence failure');
        if (state.rejectWrite) return { success: false, config: state.config };
        state.config = { ...args.config, max_entries: 49000 };
        return { success: true, config: structuredClone(state.config) };
      }
      throw new Error('Unexpected fixture IPC: ' + command);
    },
  };
});
const url = new URL(
  '/scripts/fixtures/time-travel-settings/index.html?lang=en',
  process.env.QOREDB_UI_BASE_URL ?? 'http://127.0.0.1:1430'
);
const days = page.locator('#time-travel-retention');
const save = page.getByRole('button', { name: 'Save', exact: true });
const writes = () => page.evaluate(() => window.__settings.calls.filter(c => c.command === 'update_time_travel_config'));
try {
  await page.goto(url.href);
  await days.waitFor();
  assert.equal(await days.isDisabled(), true);
  assert.equal(await save.isDisabled(), true);
  await page.evaluate(() => (window.__settings.pendingRead = false));
  await page.waitForFunction(() => !document.querySelector('#time-travel-retention').matches(':disabled'));
  assert.equal(await days.inputValue(), '30');
  assert.equal(await save.isDisabled(), true);
  console.log('PASS initial loading blocks edits and saves');

  const exclusions = page.locator('#time-travel-excluded');
  await exclusions.pressSequentially('sessions, audit_log');
  assert.equal(await exclusions.inputValue(), 'sessions, audit_log');
  assert.equal((await writes()).length, 0);
  console.log('PASS multiple excluded tables can be typed without losing the separator');

  await days.fill('7');
  await days.fill('70');
  await days.blur();
  assert.equal((await writes()).length, 0);
  await save.click();
  await page.waitForFunction(() => window.__settings.calls.some(c => c.command === 'update_time_travel_config'));
  assert.equal(await days.isDisabled(), true);
  assert.equal(await save.isDisabled(), true);
  const [request] = await writes();
  assert.equal(request.args.config.retention_days, 70);
  assert.deepEqual(request.args.config.excluded_tables, ['sessions', 'audit_log']);
  assert.deepEqual(request.args.config.sensitive_columns, ['private_note']);
  await page.evaluate(() => (window.__settings.pendingWrite = false));
  await page.waitForFunction(() => document.querySelector('#time-travel-max-entries').value === '49000');
  assert.equal((await writes()).length, 1);
  assert.equal(await save.isDisabled(), true);
  console.log('PASS draft typing sends no IPC; one explicit save preserves hidden settings and uses returned config');

  for (const failure of ['failWrite', 'rejectWrite']) {
    await page.evaluate(key => (window.__settings[key] = true), failure);
    await days.fill('60');
    await save.click();
    await page.getByRole('alert').waitFor();
    assert.equal(await days.inputValue(), '60');
    assert.equal(await save.isEnabled(), true);
    await page.evaluate(key => (window.__settings[key] = false), failure);
    await save.click();
    await page.getByRole('alert').waitFor({ state: 'detached' });
    await page.waitForFunction(() => !document.querySelector('#time-travel-retention').matches(':disabled'));
    assert.equal(await save.isDisabled(), true);
    await days.fill('70');
    await save.click();
    await page.waitForFunction(() => window.__settings.config.retention_days === 70);
  }
  console.log('PASS transport and backend refusals preserve the draft, display an error, and allow retry');

  await days.fill('-1');
  const count = (await writes()).length;
  await save.click();
  assert.equal((await writes()).length, count);
  await days.fill('366');
  await save.click();
  assert.equal((await writes()).length, count);
  console.log('PASS invalid duration is rejected before IPC');

  await page.goto(url.href + '&failRead=1');
  await page.evaluate(() => (window.__settings.pendingRead = false));
  await page.getByRole('alert').waitFor();
  assert.equal(await days.isDisabled(), true);
  assert.equal(await save.isDisabled(), true);
  await page.evaluate(() => (window.__settings.failRead = false));
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await page.waitForFunction(() => !document.querySelector('#time-travel-retention').matches(':disabled'));
  assert.equal(await days.inputValue(), '30');
  assert.equal((await writes()).length, 0);
  console.log('PASS failed initial load never saves defaults and can be retried');

  await page.goto(url.href + '&core=1');
  await page.waitForFunction(() => !!document.querySelector('main'));
  assert.equal(await days.count(), 0);
  assert.deepEqual(await page.evaluate(() => window.__settings.calls), []);
  assert.deepEqual(errors, []);
  console.log('PASS unavailable feature makes no history IPC requests; no browser errors');
} finally {
  await browser.close();
}
