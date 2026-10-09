// SPDX-License-Identifier: Apache-2.0

import assert from 'node:assert/strict';

const { chromium } = await import(process.env.QOREDB_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({ headless: true, executablePath: process.env.QOREDB_CHROMIUM_EXECUTABLE });
const page = await browser.newPage();
page.setDefaultTimeout(5000);
const errors = [];
page.on('pageerror', error => {errors.push(error.message);console.log('PAGEERROR '+error.message);});
await page.route('**/node_modules/.vite/deps/sonner.js*', route => route.fulfill({contentType:'application/javascript',body:'export const toast = Object.fromEntries(["success","error","info"].map(type => [type, message => window.__toasts.push({type,message})]));'}));
await page.route('**/node_modules/.vite/deps/@tauri-apps_plugin-fs.js*', route => route.fulfill({contentType:'application/javascript',body:`
export async function readTextFile(path) {
  const data = window.__files[path];
  if (window.__native.holdRead) await new Promise(resolve => window.__native.releaseRead = resolve);
  if (data === undefined) throw new Error('Synthetic missing file');
  return data;
}
export async function writeTextFile(path, content) { window.__writes.push({path, content}); }
`}));
await page.route('**/src/providers/LicenseProvider.tsx*', route => route.fulfill({contentType:'application/javascript',body:'export const useLicense = () => ({isFeatureEnabled:()=>true});'}));
await page.route('**/src/providers/PluginProvider.tsx*', route => route.fulfill({contentType:'application/javascript',body:'export const usePlugins = () => ({contributions:{connectionTemplates:[]}});'}));
await page.addInitScript(() => {
  window.$RefreshReg$ = () => {};
  window.$RefreshSig$ = () => type => type;
  localStorage.clear();
  window.__opened = [];
  window.__writes = [];
  window.__toasts = [];
  window.__native = {path:'/notebook.qnb',calls:[]};
  window.__files = {
    '/notebook.qnb': JSON.stringify({version:1,metadata:{id:'fixture',title:'Synthetic',createdAt:'',updatedAt:''},cells:[{id:'cell',type:'sql',source:'SELECT 1'}],variables:{}}),
    '/project.json': JSON.stringify({type:'qoredb_project',version:1,projectId:'source',exportedAt:0,credentialsIncluded:false,connections:[{id:'c',name:'Synthetic',driver:'postgres',environment:'development',read_only:true,host:'localhost',port:5432,username:'user',ssl:false}],queryLibrary:{version:1,exportedAt:0,folders:[],items:[]}}),
  };
  window.__TAURI_INTERNALS__ = {transformCallback:()=>1,invoke:async (command,args) => {
    const native = window.__native;
    native.calls.push({command,args});
    if (command === 'plugin:dialog|open') {
      if (native.holdDialog) await new Promise(resolve => native.releaseDialog = resolve);
      return native.path;
    }
    if (command === 'plugin:dialog|save') {
      if (native.holdDialog) await new Promise(resolve => native.releaseDialog = resolve);
      return '/export.json';
    }
    if (command === 'list_saved_connections') return [];
    if (command === 'save_connection') {
      if (native.holdSave) await new Promise(resolve => native.releaseSave = resolve);
      return {success:true};
    }
    if (command === 'connect_saved_connection') {
      if (native.holdConnect) await new Promise(resolve => native.releaseConnect = resolve);
      return {success:true,session_id:'session-new'};
    }
    if (command === 'disconnect') return {success:true};
    if (command === 'plugin:opener|reveal_item_in_dir') return;
    throw new Error('Unexpected IPC: '+command);
  }};
});
const url = new URL('/scripts/fixtures/file-operations/index.html?lang=en',process.env.QOREDB_UI_BASE_URL ?? 'http://127.0.0.1:1430').href;
let passed = 0;
async function reset() { await page.goto(url); await page.waitForFunction(() => window.__openNotebook); }
async function check(name, run) { await reset(); await run(); passed++; console.log('PASS '+name); }
async function startRead() {
  await page.evaluate(() => {window.__native.holdRead = true; window.__pending = window.__openNotebook();});
  await page.waitForFunction(() => window.__native.releaseRead);
}
async function releaseRead() { await page.evaluate(async () => {window.__native.releaseRead(); await window.__pending;}); }
async function startImport() {
  await page.evaluate(() => {window.__native.path = '/project.json';});
  await page.getByRole('button',{name:'Import project',exact:true}).click();
  await page.waitForFunction(() => window.__confirm.open);
}
try {
  await check('opening a valid notebook registers its data and creates one tab',async () => {
    await page.evaluate(() => window.__openNotebook());
    assert.equal(await page.evaluate(() => window.__opened.length),1);
    assert.equal(await page.evaluate(() => window.__consume('/notebook.qnb').metadata.title),'Synthetic');
  });
  for (const change of ['workspace','roundtrip','transition','session','session-roundtrip','unmount']) {
    await check('a late notebook read is discarded after '+change,async () => {
      await startRead();
      await page.evaluate(change => {
        if (change === 'workspace') window.__workspace('b');
        if (change === 'roundtrip') {window.__workspace('b');window.__workspace('a');}
        if (change === 'transition') {window.__loading(true);window.__loading(false);}
        if (change === 'session' || change === 'session-roundtrip') window.__session('session-b');
        if (change === 'unmount') window.__unmount();
      },change);
      if (change === 'session-roundtrip') await page.evaluate(() => window.__session('session-a'));
      await releaseRead();
      assert.equal(await page.evaluate(() => window.__opened.length),0);
      assert.equal(await page.evaluate(() => window.__consume('/notebook.qnb')),null);
    });
  }
  await check('same-tick requests share the single pending open operation',async () => {
    await page.evaluate(() => {window.__native.holdRead=true;window.__pending=Promise.all([window.__openNotebook(),window.__openNotebook()]);});
    await page.waitForFunction(() => window.__native.releaseRead);
    await releaseRead();
    assert.equal(await page.evaluate(() => window.__opened.length),1);
    assert.equal(await page.evaluate(() => window.__native.calls.filter(c=>c.command==='plugin:dialog|open').length),1);
  });
  await check('malformed notebook errors are visible and a later valid open succeeds',async () => {
    await page.evaluate(async () => {window.__native.path='/bad.qnb';window.__files['/bad.qnb']='null';await window.__openNotebook();});
    assert.equal(await page.evaluate(() => window.__toasts.filter(t=>t.type==='error').length),1);
    assert.equal(await page.evaluate(() => window.__opened.length),0);
    await page.evaluate(async () => {window.__native.path='/notebook.qnb';await window.__openNotebook();});
    assert.equal(await page.evaluate(() => window.__opened.length),1);
  });
  await check('cancelling the file dialog permits a subsequent open',async () => {
    await page.evaluate(async () => {window.__native.path=null;await window.__openNotebook();window.__native.path='/notebook.qnb';await window.__openNotebook();});
    assert.equal(await page.evaluate(() => window.__opened.length),1);
    assert.deepEqual(await page.evaluate(() => window.__toasts),[]);
  });
  await check('an import confirmation cannot resume after an A/B/A workspace round trip',async () => {
    await startImport();
    await page.evaluate(() => {window.__workspace('b');window.__workspace('a');window.__resolveConfirm(true);});
    await page.waitForFunction(() => !window.__confirm.open);
    await page.getByRole('button',{name:'Import project',exact:true}).waitFor();
    assert.equal(await page.evaluate(() => window.__native.calls.length),0);
  });
  await check('a late project file cannot import into a different workspace',async () => {
    await page.evaluate(() => {window.__native.holdRead=true;});
    await startImport();
    await page.evaluate(() => window.__resolveConfirm(true));
    await page.waitForFunction(() => window.__native.releaseRead);
    await page.evaluate(() => {window.__workspace('b');window.__native.releaseRead();});
    await page.waitForFunction(() => !document.querySelector('button').disabled);
    assert.equal(await page.evaluate(() => window.__native.calls.filter(c=>c.command==='save_connection').length),0);
    assert.equal(await page.evaluate(() => Object.keys(localStorage).filter(key=>key.startsWith('qoredb_query_library')).length),0);
  });
  await check('a pending export dialog cannot write after the workspace changes',async () => {
    await page.evaluate(() => {window.__native.holdDialog=true;});
    await page.getByRole('button',{name:'Export project',exact:true}).click();
    await page.waitForFunction(() => window.__native.releaseDialog);
    await page.evaluate(() => {window.__workspace('b');window.__native.releaseDialog();});
    await page.waitForFunction(() => !document.querySelector('button').disabled);
    assert.deepEqual(await page.evaluate(() => window.__writes),[]);
  });
  await check('a valid project import succeeds with one connection and disables overlapping transfer actions',async () => {
    await startImport();
    assert.equal(await page.getByRole('button',{name:'Export project',exact:true}).isDisabled(),true);
    await page.evaluate(() => window.__resolveConfirm(true));
    await page.waitForFunction(() => window.__toasts.some(t=>t.type==='success'));
    assert.equal(await page.evaluate(() => window.__native.calls.filter(c=>c.command==='save_connection').length),1);
    await page.waitForFunction(() => !document.querySelector('button').disabled);
    assert.equal(await page.getByRole('button',{name:'Import project',exact:true}).isDisabled(),false);
  });
  await check('editing a connection sends its active workspace to the backend',async () => {
    await page.evaluate(() => window.__connection({edit:true}));
    await page.getByRole('button',{name:'Save Changes',exact:true}).click();
    await page.waitForFunction(() => window.__saved);
    assert.equal(await page.evaluate(() => window.__native.calls.find(c=>c.command==='save_connection').args.input.project_id),'a');
    assert.equal(await page.evaluate(() => window.__saved.project_id),'a');
  });
  await check('a pending connection save cannot submit callbacks into the next workspace',async () => {
    await page.evaluate(() => {window.__native.holdSave=true;window.__connection({edit:true});});
    await page.getByRole('button',{name:'Save Changes',exact:true}).click();
    await page.waitForFunction(() => window.__native.releaseSave);
    await page.evaluate(() => {window.__workspace('b');window.__native.releaseSave();});
    await page.getByRole('dialog').waitFor({state:'hidden'});
    assert.equal(await page.evaluate(() => window.__saved),undefined);
    assert.equal(await page.evaluate(() => window.__connected),undefined);
  });
  for (const interrupted of [false,true]) {
    await check(interrupted ? 'a workspace switch during creation cannot connect the saved connection in the next project' : 'creating and connecting uses the active project for both commands',async () => {
      await page.evaluate(interrupted => {window.__native.holdSave=interrupted;window.__connection({edit:false});},interrupted);
      await page.getByRole('button',{name:/PostgreSQL/}).click();
      await page.getByPlaceholder('user',{exact:true}).fill('user');
      await page.getByRole('button',{name:'Save & Connect',exact:true}).click();
      if (interrupted) {
        await page.waitForFunction(() => window.__native.releaseSave);
        await page.evaluate(() => {window.__workspace('b');window.__native.releaseSave();});
        await page.getByRole('dialog').waitFor({state:'hidden'});
        assert.equal(await page.evaluate(() => window.__native.calls.filter(c=>c.command==='connect_saved_connection').length),0);
        assert.equal(await page.evaluate(() => window.__connected),undefined);
      } else {
        await page.waitForFunction(() => window.__connected);
        assert.equal(await page.evaluate(() => window.__native.calls.find(c=>c.command==='save_connection').args.input.project_id),'a');
        assert.equal(await page.evaluate(() => window.__native.calls.find(c=>c.command==='connect_saved_connection').args.projectId),'a');
        assert.equal(await page.evaluate(() => window.__connected.value.project_id),'a');
      }
    });
  }
  await check('a session created after the form becomes stale is disconnected',async () => {
    await page.evaluate(() => {window.__native.holdConnect=true;window.__connection({edit:false});});
    await page.getByRole('button',{name:/PostgreSQL/}).click();
    await page.getByPlaceholder('user',{exact:true}).fill('user');
    await page.getByRole('button',{name:'Save & Connect',exact:true}).click();
    await page.waitForFunction(() => window.__native.releaseConnect);
    await page.evaluate(() => {window.__workspace('b');window.__native.releaseConnect();});
    await page.waitForFunction(() => window.__native.calls.some(c=>c.command==='disconnect'));
    assert.equal(await page.evaluate(() => window.__native.calls.find(c=>c.command==='disconnect').args.sessionId),'session-new');
    assert.equal(await page.evaluate(() => window.__connected),undefined);
  });
  assert.deepEqual(errors,[]);
  console.log(`${passed} file operation scenarios passed (Chromium, simulated native IPC).`);
} finally { await browser.close(); }
