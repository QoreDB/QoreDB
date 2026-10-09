// SPDX-License-Identifier: Apache-2.0

import assert from 'node:assert/strict';

const { chromium } = await import(process.env.QOREDB_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({ headless: true, executablePath: process.env.QOREDB_CHROMIUM_EXECUTABLE });
const page = await browser.newPage();
page.setDefaultTimeout(5000);
const errors = [];
page.on('pageerror', error => { errors.push(error.message); console.log('PAGEERROR ' + error.message); });
await page.route('**/node_modules/.vite/deps/sonner.js*', route => route.fulfill({
  contentType: 'application/javascript',
  body: 'export const toast = Object.fromEntries(["success","error","info"].map(type => [type, (message, options) => window.__toasts.push({type,message,...options})]));',
}));
await page.route('**/src/providers/LicenseProvider.tsx*', route => route.fulfill({
  contentType: 'application/javascript', body: 'export const useLicense = () => ({isFeatureEnabled:()=>true});',
}));
await page.route('**/node_modules/.vite/deps/@tauri-apps_plugin-fs.js*', route => route.fulfill({
  contentType: 'application/javascript', body: `export async function readTextFile() {
    if (window.__backend.holdRead) await new Promise(resolve => window.__backend.releaseRead = resolve);
    window.__backend.readReturned = true;
    return JSON.stringify({version:1, folders:[], items:[{title:'Imported A', query:'SELECT 42', tags:[]}]});
  }
  export async function writeTextFile() {}`,
}));
await page.addInitScript(() => {
  window.$RefreshReg$ = () => {};
  window.$RefreshSig$ = () => type => type;
  if (!location.search.includes('keep=1')) localStorage.clear();
  window.__toasts = [];
  const info = project => ({ path: `/${project}/.qoredb`, source: project === 'default' ? 'default' : 'manual', manifest: { name: project, version: 1, created_at: '', updated_at: '' } });
  window.__backend = { project: 'a', calls: [], files: { a: { version: 1, folders: [], items: [] }, b: { version: 1, folders: [], items: [] } } };
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 1,
    invoke: async (command, args) => {
      const backend = window.__backend;
      backend.calls.push({ command, args });
      if (command === 'plugin:dialog|open') return '/synthetic-library.json';
      if (command === 'plugin:event|listen') return 1;
      if (command === 'plugin:event|unlisten') return;
      if (command === 'detect_workspace') return null;
      if (command === 'get_active_workspace') return info(backend.project);
      if (command === 'get_workspace_project_id') return backend.project;
      if (command === 'list_recent_workspaces') return [];
      if (command === 'ws_get_query_library' || command === 'ws_save_query_library') {
        if (args.projectId !== backend.project) throw new Error('Workspace mismatch');
        if (command === 'ws_get_query_library') {
          const data = backend.files[backend.project];
          if (backend.holdGet) await new Promise(resolve => backend.releaseGet = resolve);
          return data;
        }
        if (backend.holdSave) await new Promise(resolve => backend.releaseSave = resolve);
        if (backend.failSave) throw new Error('Synthetic disk failure');
        backend.files[backend.project] = args.library;
        return true;
      }
      if (['switch_workspace', 'open_workspace', 'create_workspace', 'switch_to_default_workspace'].includes(command)) {
        backend.project = command === 'switch_to_default_workspace' ? 'default' : args?.qoredbPath === '/a/.qoredb' ? 'a' : 'b';
        if (command === 'switch_to_default_workspace') return info('default');
        return { success: true, workspace: info(backend.project) };
      }
      throw new Error('Unexpected IPC: ' + command);
    },
  };
});
const url = new URL('/scripts/fixtures/query-library/index.html?lang=en', process.env.QOREDB_UI_BASE_URL ?? 'http://127.0.0.1:1430').href;
const transitions = ['switchWorkspace', 'openWorkspace', 'createWorkspace', 'switchToDefault'];
let passed = 0;
async function reset() {
  await page.goto(url);
  await page.waitForFunction(() => window.__workspace?.projectId === 'a' && !window.__workspace.isLoading && window.__backend.calls.some(c => c.command === 'ws_get_query_library'));
  await page.evaluate(() => {
    window.__library.addItem({ title: 'Keep A', query: 'SELECT 9007199254740993' });
    window.__backend.calls = [];
  });
}
try {
  for (const method of transitions) {
    await reset();
    await page.evaluate(method => {
      window.__backend.holdSave = true;
      window.__transition = window.__workspace[method]('/b/.qoredb', 'B');
    }, method);
    await page.waitForFunction(() => window.__backend.releaseSave);
    assert.equal(await page.evaluate(() => window.__backend.project), 'a');
    assert.equal(await page.evaluate(() => window.__backend.calls.length), 1);
    await page.evaluate(async () => { window.__backend.releaseSave(); await window.__transition; });
    const state = await page.evaluate(() => window.__backend);
    assert.equal(state.files.a.items[0].query, 'SELECT 9007199254740993');
    assert.deepEqual(state.files.b.items, []);
    assert.equal(state.project, method === 'switchToDefault' ? 'default' : 'b');
    passed++;
    console.log('PASS ' + method + ' waits for saving A and leaves B intact');

    await reset();
    await page.evaluate(async method => {
      window.__backend.failSave = true;
      await window.__workspace[method]('/b/.qoredb', 'B');
    }, method);
    assert.equal(await page.evaluate(() => window.__backend.project), 'a');
    assert.equal(await page.evaluate(() => window.__library.listItems()[0].title), 'Keep A');
    assert.equal(await page.evaluate(() => window.__toasts.filter(t => t.type === 'error').length), 1);
    await page.evaluate(async method => {
      window.__backend.failSave = false;
      await window.__workspace[method]('/b/.qoredb', 'B');
    }, method);
    assert.equal(await page.evaluate(() => window.__backend.files.a.items[0].title), 'Keep A');
    passed++;
    console.log('PASS ' + method + ' stays on A after failure and retries successfully');
  }
  await reset();
  await page.evaluate(() => {
    window.__backend.holdSave = true;
    window.__transition = window.__workspace.switchWorkspace('/b/.qoredb');
    window.__duplicate = window.__workspace.openWorkspace('/b/.qoredb');
  });
  await page.waitForFunction(() => window.__backend.releaseSave);
  assert.equal(await page.evaluate(() => window.__duplicate), false);
  assert.equal(await page.evaluate(() => {
    try { window.__library.addItem({ title: 'During transition', query: 'SELECT 2' }); return 'accepted'; }
    catch { return 'blocked'; }
  }), 'blocked');
  await page.evaluate(async () => { window.__backend.releaseSave(); await window.__transition; });
  assert.equal(await page.evaluate(() => window.__backend.calls.filter(c => ['switch_workspace', 'open_workspace'].includes(c.command)).length), 1);
  passed++;
  console.log('PASS concurrent transitions and edits cannot bypass the pending save');
  await reset();
  await page.goto(url + '&keep=1');
  await page.waitForFunction(() => window.__backend.files.a.items.length === 1);
  assert.equal(await page.evaluate(() => window.__backend.files.a.items[0].query), 'SELECT 9007199254740993');
  passed++;
  console.log('PASS pending local edits survive a fresh frontend session and resync on activation');
  await reset();
  await page.evaluate(() => {
    const item = window.__library.listItems()[0];
    window.__backend.files.b.items = [{...item, title:'Keep B'}];
    window.__dialogs.library(true);
  });
  await page.getByText('Keep A', { exact: true }).waitFor();
  await page.getByTitle('Delete', { exact: true }).click();
  await page.waitForFunction(() => window.__confirm.open);
  await page.evaluate(() => window.__workspace.switchWorkspace('/b/.qoredb'));
  await page.evaluate(() => window.__resolveConfirm(true));
  assert.equal(await page.evaluate(() => window.__library.listItems().length), 1);
  await page.getByText('Keep B', { exact: true }).waitFor();
  assert.equal(await page.getByText('Keep A', { exact: true }).count(), 0);
  passed++;
  console.log('PASS a pending delete cannot remove the same ID in another project; the view reloads B');

  await reset();
  await page.evaluate(() => { window.__backend.holdRead = true; window.__dialogs.library(true); });
  await page.getByRole('button', { name: 'Import', exact: true }).click();
  await page.waitForFunction(() => window.__backend.releaseRead);
  await page.evaluate(() => window.__workspace.switchWorkspace('/b/.qoredb'));
  await page.evaluate(() => window.__backend.releaseRead());
  await page.waitForFunction(() => window.__backend.readReturned);
  assert.equal(await page.evaluate(() => window.__library.listItems().length), 0);
  passed++;
  console.log('PASS a late import never inserts A data into B');

  await reset();
  await page.evaluate(() => window.__dialogs.save(true));
  await page.getByRole('dialog').waitFor();
  await page.evaluate(() => window.__workspace.switchWorkspace('/b/.qoredb'));
  assert.equal(await page.getByRole('dialog').count(), 0);
  assert.equal(await page.evaluate(() => window.__library.listItems().length), 0);
  passed++;
  console.log('PASS the save dialog closes when its originating project changes');
  await reset();
  await page.evaluate(() => { window.__backend.holdRead = true; window.__dialogs.library(true); });
  await page.getByRole('button', { name: 'Import', exact: true }).click();
  await page.waitForFunction(() => window.__backend.releaseRead);
  await page.evaluate(() => window.__dialogs.library(false));
  await page.getByText('Keep A', { exact: true }).waitFor({ state: 'hidden' });
  await page.evaluate(() => window.__dialogs.library(true));
  await page.getByText('Keep A', { exact: true }).waitFor();
  await page.evaluate(() => window.__backend.releaseRead());
  await page.waitForFunction(() => window.__backend.readReturned);
  assert.equal(await page.evaluate(() => window.__library.listItems().length), 1);
  passed++;
  console.log('PASS closing and reopening does not revive a pending import');

  await reset();
  await page.evaluate(() => window.__dialogs.library(true));
  await page.getByTitle('Delete', { exact: true }).click();
  await page.waitForFunction(() => window.__confirm.open);
  await page.evaluate(async () => {
    await window.__workspace.switchWorkspace('/b/.qoredb');
  });
  await page.waitForFunction(() => window.__workspace.projectId === 'b');
  await page.evaluate(() => window.__workspace.switchWorkspace('/a/.qoredb'));
  await page.evaluate(() => window.__resolveConfirm(true));
  assert.equal(await page.evaluate(() => window.__library.listItems().length), 1);
  passed++;
  console.log('PASS returning to A does not revive an old deletion confirmation');

  await reset();
  await page.evaluate(() => {
    const item = window.__library.listItems()[0];
    window.__library.updateItem(item.id, {query: 'SELECT $n', variables: {n: {name:'n',type:'number',defaultValue:'42'}}});
    window.__dialogs.library(true);
  });
  await page.getByTitle('Use query', { exact: true }).click();
  await page.getByRole('dialog').waitFor();
  await page.evaluate(() => window.__workspace.switchWorkspace('/b/.qoredb'));
  assert.equal(await page.getByRole('dialog').count(), 0);
  assert.equal(await page.evaluate(() => window.__selectedQuery), undefined);
  passed++;
  console.log('PASS the parameter prompt cannot submit a query from the previous project');

  await reset();
  await page.evaluate(() => {
    const item = window.__library.listItems()[0];
    localStorage.setItem('qoredb_query_library_v1_a', JSON.stringify({folders:[],items:Array.from({length:300}, (_,i) => ({...item,id:String(i)}))}));
    window.__dialogs.save(true);
  });
  await page.getByRole('button', {name:'Save', exact:true}).click();
  assert.equal(await page.evaluate(() => window.__library.listItems().length), 300);
  assert.equal(await page.getByRole('dialog').count(), 1);
  assert.match(await page.evaluate(() => window.__toasts.at(-1).description), /limited to 300 queries/);
  passed++;
  console.log('PASS capacity error is visible and the save draft stays open');
  await reset();
  await page.evaluate(() => {
    const item = window.__library.listItems()[0];
    window.__backend.files.b.items = [{...item, title:'Loaded later in B'}];
    window.__dialogs.library(true);
  });
  await page.getByText('Keep A', {exact:true}).waitFor();
  await page.evaluate(() => {
    window.__backend.holdGet = true;
    window.__transition = window.__workspace.switchWorkspace('/b/.qoredb');
  });
  await page.waitForFunction(() => window.__backend.releaseGet);
  assert.equal(await page.getByText('Keep A', {exact:true}).count(), 0);
  await page.evaluate(async () => { window.__backend.releaseGet(); await window.__transition; });
  await page.getByText('Loaded later in B', {exact:true}).waitFor();
  passed++;
  console.log('PASS an already visible library refreshes after a delayed disk read');
  assert.deepEqual(errors, []);
  console.log(`${passed} workspace library scenarios passed (Chromium, mocked native IPC).`);
} finally {
  await browser.close();
}
