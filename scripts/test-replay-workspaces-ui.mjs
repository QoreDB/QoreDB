// SPDX-License-Identifier: BUSL-1.1

import assert from 'node:assert/strict';

const { chromium } = await import(process.env.QOREDB_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({ headless: true, executablePath: process.env.QOREDB_CHROMIUM_EXECUTABLE });
const page = await browser.newPage();
page.setDefaultTimeout(6000);
const base = process.env.QOREDB_UI_BASE_URL ?? 'http://127.0.0.1:1430';
const failures = [];
const errors = [];
page.on('pageerror', error => errors.push(error.message));
await page.route('**/src/providers/LicenseProvider.tsx*', route => route.fulfill({ contentType: 'application/javascript', body: 'export const useLicense = () => ({isFeatureEnabled: () => true});' }));
await page.route('**/src/lib/notify.tsx*', route => route.fulfill({ contentType: 'application/javascript', body: 'export const notify = {error: (...args) => window.__notices.push(args), success: (...args) => window.__notices.push(args)};' }));
await page.addInitScript(() => {
  window.__backendProject = 'default';
  window.__notices = [];
  window.__calls = [];
  window.__pending = [];
  window.__holdStatus = false;
  window.__holdAction = false;
  window.__status = {run_id: 'run-a', name: 'Private recording A', entry_count: 1, excluded_mutations: 0, mutation_count: 0};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 1,
    invoke: async (command, args) => {
      window.__calls.push({command, args});
      if (command === 'replay_recording_status') {
        const response = args.projectId === window.__backendProject && args.projectId === 'default' ? window.__status : null;
        if (window.__holdStatus) {
          window.__holdStatus = false;
          return new Promise(resolve => window.__pending.push(() => resolve(response)));
        }
        return response;
      }
      if (command === 'replay_recorded_previews') {
        const response = [{order: 1, query_preview: 'SELECT synthetic_private', is_mutation: false, looks_like_secret: false}];
        if (window.__holdPreviews) {
          window.__holdPreviews = false;
          return new Promise(resolve => window.__pending.push(() => resolve(response)));
        }
        return response;
      }
      if (['replay_stop_recording', 'replay_cancel_recording', 'replay_discard_recorded', 'replay_discard_mutations'].includes(command)) {
        const response = {slug: 'saved-a', name: 'Private recording A'};
        if (window.__holdAction) return new Promise((resolve, reject) => {
          window.__resolveAction = () => resolve(response);
          window.__rejectAction = () => reject(new Error('Private failure A'));
        });
        window.__status = null;
        return response;
      }
      if (command === 'replay_start_recording') return window.__status;
      if (command === 'replay_load_set') return {name: args.slug, entries: []};
      if (command === 'replay_last_report') return {};
      if (command === 'plugin:event|listen') return 1;
      if (command === 'plugin:event|unlisten') return null;
      if (['replay_list_sets', 'replay_list_runs', 'list_sessions'].includes(command)) return [];
      throw new Error('Unexpected IPC ' + command);
    },
  };
  window.$RefreshReg$ = () => {};
  window.$RefreshSig$ = () => type => type;
});
async function scenario(name, action, fixture = 'indicator') {
  await page.goto(new URL(`/scripts/fixtures/replay/${fixture}.html?lang=en`, base).href);
  try { await action(); console.log('PASS ' + name); }
  catch (error) { failures.push(name + ': ' + error.message); console.log('FAIL ' + name); console.log(JSON.stringify(await page.evaluate(() => ({calls: window.__calls, body: document.body.innerText})))); }
}
async function openIndicator() {
  await page.getByRole('button', {name: /recording/i}).click();
  await page.getByText('Private recording A', {exact: true}).waitFor();
}
try {
  await scenario('workspace switch immediately hides the recording and return restores it', async () => {
    await openIndicator();
    await page.evaluate(() => window.__switchWorkspace('other'));
    await page.waitForTimeout(60);
    assert.equal(await page.getByText('Private recording A', {exact: true}).count(), 0);
    assert.equal(await page.getByRole('button', {name: /recording/i}).count(), 0);
    await page.evaluate(() => window.__switchWorkspace('default'));
    await openIndicator();
  });
  await scenario('late status cannot reappear in another workspace', async () => {
    await openIndicator();
    await page.evaluate(() => { window.__holdStatus = true; });
    await page.waitForFunction(() => window.__pending.length === 1);
    await page.evaluate(() => window.__switchWorkspace('other'));
    await page.evaluate(() => window.__pending.shift()());
    await page.waitForTimeout(60);
    assert.equal(await page.getByRole('button', {name: /recording/i}).count(), 0);
  });
  for (const reject of [false, true]) {
    await scenario(`late ${reject ? 'failure' : 'success'} cannot notify another workspace`, async () => {
      await openIndicator();
      await page.evaluate(() => { window.__holdAction = true; });
      await page.getByRole('button', {name: 'Stop and save', exact: true}).click();
      await page.waitForFunction(() => Boolean(window.__resolveAction));
      await page.evaluate(() => window.__switchWorkspace('other'));
      await page.evaluate(reject => reject ? window.__rejectAction() : window.__resolveAction(), reject);
      await page.waitForTimeout(60);
      assert.deepEqual(await page.evaluate(() => window.__notices), []);
      const call = await page.evaluate(() => window.__calls.find(call => call.command === 'replay_stop_recording'));
      assert.equal(call.args.projectId, 'default');
      assert.equal(call.args.runId, 'run-a');
    });
  }
  await scenario('cancel binds the exact workspace and recording', async () => {
    await openIndicator();
    await page.getByRole('button', {name: 'Discard the recording', exact: true}).click();
    await page.waitForTimeout(60);
    const call = await page.evaluate(() => window.__calls.find(call => call.command === 'replay_cancel_recording'));
    assert.equal(call.args.projectId, 'default');
    assert.equal(call.args.runId, 'run-a');
  });
  await scenario('failed cancel remains visible and can be retried in the origin', async () => {
    await openIndicator();
    await page.evaluate(() => { window.__holdAction = true; });
    await page.getByRole('button', {name: 'Discard the recording', exact: true}).click();
    await page.waitForFunction(() => Boolean(window.__rejectAction));
    await page.evaluate(() => window.__rejectAction());
    await page.waitForFunction(() => window.__notices.length === 1);
    await openIndicator();
    await page.evaluate(() => { window.__holdAction = false; });
    await page.getByRole('button', {name: 'Discard the recording', exact: true}).click();
    await page.waitForFunction(() => window.__status === null);
    await page.getByRole('button', {name: /recording/i}).waitFor({state: 'hidden'});
  });
  await scenario('returning to the origin does not revive its old pending response', async () => {
    await openIndicator();
    await page.evaluate(() => { window.__holdStatus = true; });
    await page.waitForFunction(() => window.__pending.length === 1);
    await page.evaluate(() => window.__switchWorkspace('other'));
    await page.getByRole('button', {name: /recording/i}).waitFor({state: 'hidden'});
    await page.evaluate(() => {
      window.__status = {...window.__status, run_id: 'run-b', name: 'New recording A'};
      window.__switchWorkspace('default');
    });
    await page.getByRole('button', {name: /recording/i}).click();
    await page.getByText('New recording A', {exact: true}).waitFor();
    await page.evaluate(() => window.__pending.shift()());
    await page.waitForTimeout(60);
    assert.equal(await page.getByText('Private recording A', {exact: true}).count(), 0);
    await page.getByText('New recording A', {exact: true}).waitFor();
  });
  for (const [action, command] of [
    ['endRecording', 'replay_stop_recording'],
    ['abortRecording', 'replay_cancel_recording'],
    ['dropRecorded', 'replay_discard_recorded'],
    ['dropMutations', 'replay_discard_mutations'],
  ]) {
    await scenario(`hook ${action} binds workspace and recording`, async () => {
      await page.waitForFunction(() => window.__replay?.recording?.run_id === 'run-a');
      await page.evaluate(action => window.__replay[action](0), action);
      const call = await page.evaluate(command => window.__calls.find(c => c.command === command), command);
      assert.equal(call.args.projectId, 'default');
      assert.equal(call.args.runId, 'run-a');
      assert.deepEqual(await page.evaluate(() => window.__notices.filter(n => n.join(' ').includes('Unexpected'))), []);
    }, 'hook');
  }
  await scenario('hook start binds its workspace', async () => {
    await page.waitForFunction(() => Boolean(window.__replay));
    await page.evaluate(() => window.__replay.beginRecording({name: 'Synthetic', ignoredColumns: [], recordMutations: false, captureMode: 'metadata_only', allowProductionCapture: false}));
    const call = await page.evaluate(() => window.__calls.find(c => c.command === 'replay_start_recording'));
    assert.equal(call.args.request.project_id, 'default');
    assert.equal(call.args.request.session_id, 'synthetic-session');
  }, 'hook');
  await scenario('hook clears previews and blocks old recording controls after switching workspace', async () => {
    await page.waitForFunction(() => window.__replay?.previews?.length === 1);
    await page.evaluate(() => {
      window.__oldReplay = window.__replay;
      window.__backendProject = 'other';
      window.__switchContext('other-session', 'other');
    });
    await page.waitForFunction(() => window.__replay.recording === null && window.__replay.previews.length === 0);
    await page.evaluate(async () => {
      await window.__oldReplay.abortRecording();
      await window.__oldReplay.endRecording();
      await window.__oldReplay.dropRecorded(0);
      await window.__oldReplay.dropMutations();
    });
    assert.equal(await page.evaluate(() => window.__calls.some(c => ['replay_stop_recording', 'replay_cancel_recording', 'replay_discard_recorded', 'replay_discard_mutations'].includes(c.command))), false);
  }, 'hook');
  await scenario('hook ignores a recording response arriving after workspace switch', async () => {
    await page.waitForFunction(() => window.__replay?.recording?.run_id === 'run-a');
    await page.evaluate(() => { window.__holdStatus = true; });
    await page.waitForFunction(() => window.__pending.length === 1);
    await page.evaluate(() => {
      window.__backendProject = 'other';
      window.__switchContext('other-session', 'other');
    });
    await page.waitForFunction(() => window.__replay.recording === null);
    await page.evaluate(() => window.__pending.shift()());
    await page.waitForTimeout(60);
    assert.equal(await page.evaluate(() => window.__replay.recording), null);
    assert.deepEqual(await page.evaluate(() => window.__replay.previews), []);
  }, 'hook');
  await scenario('hook ignores private previews arriving after workspace switch', async () => {
    await page.waitForFunction(() => window.__replay?.recording?.run_id === 'run-a');
    await page.evaluate(() => { window.__holdPreviews = true; });
    await page.waitForFunction(() => window.__pending.length === 1);
    await page.evaluate(() => {
      window.__backendProject = 'other';
      window.__switchContext('other-session', 'other');
    });
    await page.waitForFunction(() => window.__replay.recording === null);
    await page.evaluate(() => window.__pending.shift()());
    await page.waitForTimeout(60);
    assert.deepEqual(await page.evaluate(() => window.__replay.previews), []);
  }, 'hook');
  assert.deepEqual(errors, []);
  assert.deepEqual(failures, []);
} finally { await browser.close(); }
