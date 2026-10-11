// SPDX-License-Identifier: BUSL-1.1

import assert from 'node:assert/strict';

const { chromium } = await import(process.env.QOREDB_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({headless:true,executablePath:process.env.QOREDB_CHROMIUM_EXECUTABLE});
const page = await browser.newPage();
page.setDefaultTimeout(5000);
const errors=[];
page.on('pageerror',error=>{errors.push(error.message);console.log('PAGEERROR '+error.message);});
await page.route('**/node_modules/.vite/deps/sonner.js*',route=>route.fulfill({contentType:'application/javascript',body:'export const toast = Object.fromEntries(["success","error","info"].map(type => [type,message => window.__toasts.push({type,message})]));'}));
await page.route('**/src/providers/LicenseProvider.tsx*',route=>route.fulfill({contentType:'application/javascript',body:'export const useLicense = () => ({isFeatureEnabled:()=>true});'}));
await page.route('**/src/providers/WorkspaceProvider.tsx*',route=>route.fulfill({contentType:'application/javascript',body:'import {useWorkspaceStore} from "/src/lib/stores/workspaceStore.ts"; export const useWorkspace = () => useWorkspaceStore(state=>state);'}));
await page.route('**/src/providers/SessionProvider.tsx*',route=>route.fulfill({contentType:'application/javascript',body:'export const useSessionContext = () => ({savedConnections:window.__maskConnections??window.__connections,refreshSidebar:()=>window.__events.push({type:"refresh"})});'}));
await page.addInitScript(()=>{
  window.$RefreshReg$ = ()=>{};
  window.$RefreshSig$ = ()=>type=>type;
  localStorage.clear();
  window.__calls=[];window.__events=[];window.__toasts=[];window.__pending=[];
  window.__releaseAll=()=>{const pending=window.__pending.splice(0);pending.forEach(resolve=>resolve());};
  window.__TAURI_INTERNALS__={transformCallback:()=>1,invoke:async(command,args)=>{
    if(command==='agents_mcp_status') return {path:null,version:null};
    if(command==='get_safety_policy') return {success:false};
    if(command==='list_saved_connections') {
      const data = structuredClone(window.__connections);
      if(window.__holdList) await new Promise(resolve=>{window.__listPending ??= [];window.__listPending.push(resolve);});
      if(window.__failList) throw new Error('Synthetic list failure');
      return data;
    }
    window.__calls.push({command,args});
    if(window.__hold) await new Promise(resolve=>window.__pending.push(resolve));
    if(window.__fail) return {success:false,error:'Synthetic failure'};
    if(command==='get_connection_credentials') return {success:true,password:'synthetic-secret'};
    if(command==='duplicate_saved_connection') return {success:true,connection:{...window.__connections[0],id:'copy'}};
    if(['test_saved_connection','delete_saved_connection','set_connection_exposed','set_connection_masking'].includes(command)) return {success:true};
    throw new Error('Unexpected IPC: '+command);
  }};
});
const url=new URL('/scripts/fixtures/vault-workspaces/index.html?lang=en',process.env.QOREDB_UI_BASE_URL??'http://127.0.0.1:1430').href;
let passed=0;
async function reset(){await page.goto(url);await page.waitForFunction(()=>window.__actions&&window.__mask);}
async function check(name,fn){await reset();await fn();passed++;console.log('PASS '+name);}
const commands={handleTest:'test_saved_connection',handleEdit:'get_connection_credentials',handleDelete:'delete_saved_connection',handleDuplicate:'duplicate_saved_connection'};
try {
  for(const [action,command] of Object.entries(commands)) {
    await check(action+' uses the active project even with legacy default metadata',async()=>{
      await page.evaluate(action=>window.__actions[action](),action);
      assert.equal(await page.evaluate(()=>window.__calls[0].args.projectId),'a');
      assert.equal(await page.evaluate(()=>window.__calls[0].command),command);
      assert.equal(await page.evaluate(()=>window.__events.length>0),true);
    });
    await check(action+' ignores late callbacks after a workspace switch',async()=>{
      await page.evaluate(action=>{window.__hold=true;window.__operation=window.__actions[action]();},action);
      await page.waitForFunction(()=>window.__pending.length===1);
      await page.evaluate(async()=>{window.__workspace('b');window.__releaseAll();await window.__operation;});
      assert.deepEqual(await page.evaluate(()=>window.__events),[]);
      assert.deepEqual(await page.evaluate(()=>window.__toasts),[]);
    });
  }
  await check('old actions cannot resume after A/B/A or while switching',async()=>{
    await page.evaluate(async()=>{const old=window.__actions.handleDelete;window.__workspace('b');window.__workspace('a');await old();window.__loading(true);await window.__actions.handleDuplicate();});
    assert.deepEqual(await page.evaluate(()=>window.__calls),[]);
  });
  await check('late credentials are discarded when their owner unmounts',async()=>{
    await page.evaluate(()=>{window.__hold=true;window.__operation=window.__actions.handleEdit();});
    await page.waitForFunction(()=>window.__pending.length===1);
    await page.evaluate(()=>window.__unmount());
    await page.getByText('Closed',{exact:true}).waitFor();
    await page.evaluate(async()=>{window.__releaseAll();await window.__operation;});
    assert.deepEqual(await page.evaluate(()=>window.__events),[]);
  });
  await check('duplicate clicks dispatch once and failures allow retry',async()=>{
    await page.evaluate(()=>{window.__hold=true;window.__operation=Promise.all([window.__actions.handleDuplicate(),window.__actions.handleDuplicate()]);});
    await page.waitForFunction(()=>window.__pending.length===1);
    assert.equal(await page.evaluate(()=>window.__calls.length),1);
    await page.evaluate(async()=>{window.__fail=true;window.__releaseAll();await window.__operation;window.__hold=false;window.__fail=false;await window.__actions.handleDuplicate();});
    assert.equal(await page.evaluate(()=>window.__calls.length),2);
    assert.equal(await page.evaluate(()=>window.__toasts.filter(t=>t.type==='error').length),1);
    assert.equal(await page.evaluate(()=>window.__toasts.filter(t=>t.type==='success').length),1);
  });
  for(const kind of ['menu','context']) {
    for(const switched of [false,true]) {
      await check(kind+' delete confirmation '+(switched?'cannot cross A/B/A':'deletes only its original project'),async()=>{
        if(kind==='menu') await page.getByTestId('menu').getByRole('button').click();
        else await page.getByTestId('context').click({button:'right'});
        await page.getByRole('menuitem',{name:'Delete',exact:true}).click();
        await page.getByRole('dialog').waitFor();
        if(switched) await page.evaluate(()=>{window.__workspace('b');window.__workspace('a');});
        await page.getByRole('dialog').getByRole('button',{name:'Delete',exact:true}).click();
        await page.getByRole('dialog').waitFor({state:'hidden'});
        const calls=await page.evaluate(()=>window.__calls.filter(c=>c.command==='delete_saved_connection'));
        assert.equal(calls.length,switched?0:1);
        if(!switched) assert.equal(calls[0].args.projectId,'a');
      });
    }
  }
  for(const switched of [false,true]) {
    await check('agent exposure '+(switched?'ignores a late result':'updates the current project'),async()=>{
      await page.evaluate(()=>{window.__hold=true;});
      await page.getByRole('switch',{name:'Allow access to this connection',exact:true}).click();
      await page.waitForFunction(()=>window.__pending.length===1);
      assert.equal(await page.evaluate(()=>window.__calls[0].args.projectId),'a');
      if(switched) await page.evaluate(()=>window.__workspace('b'));
      await page.evaluate(()=>window.__releaseAll());
      await page.waitForFunction(()=>!document.querySelector('[role="switch"]').disabled);
      assert.equal(await page.evaluate(()=>window.__events.filter(e=>e.type==='refresh').length),switched?0:1);
      assert.deepEqual(await page.evaluate(()=>window.__toasts),[]);
    });
  }
  for(const change of ['none','workspace','roundtrip','table']) {
    await check('production mask removal respects confirmation and '+change+' context',async()=>{
      await page.evaluate(()=>window.__mask.toggle('secret'));
      await page.getByRole('dialog').waitFor();
      assert.equal(await page.getByRole('button',{name:'Remove masking',exact:true}).isDisabled(),true);
      await page.getByPlaceholder('SYNTHETIC',{exact:true}).fill('SYNTHETIC');
      if(change==='workspace') await page.evaluate(()=>window.__workspace('b'));
      // Reuse the cached object so A/B/A must invalidate the confirmation by activation.
      if(change==='roundtrip') await page.evaluate(()=>{const cached=window.__connections;window.__workspace('b');window.__workspace('a');window.__connections=cached;});
      if(change==='table') await page.evaluate(()=>window.__table('other'));
      if(change==='workspace'||change==='table') await page.getByRole('dialog').waitFor({state:'hidden'});
      else await page.getByRole('button',{name:'Remove masking',exact:true}).click();
      const calls=await page.evaluate(()=>window.__calls.filter(c=>c.command==='set_connection_masking'));
      assert.equal(calls.length,change==='none'?1:0);
      if(change==='none') {assert.equal(calls[0].args.projectId,'a');assert.deepEqual(calls[0].args.masking.rules,[]);}
    });
  }
  await check('masking cannot overwrite an unavailable policy and recovers after reload',async()=>{
    await page.evaluate(()=>{window.__maskConnections=[];window.__table('other');});
    await page.waitForFunction(()=>window.__mask.canAdd===false);
    await page.evaluate(()=>window.__mask.toggle('new-column'));
    assert.deepEqual(await page.evaluate(()=>window.__calls),[]);
    await page.evaluate(()=>{window.__maskConnections=undefined;window.__table('users');});
    await page.waitForFunction(()=>window.__mask.canAdd===true);
    await page.evaluate(()=>window.__mask.toggle('new-column'));
    await page.waitForFunction(()=>window.__calls.length===1);
    assert.equal(await page.evaluate(()=>window.__calls[0].args.masking.rules.length),2);
  });
  await check('a late mask update cannot refresh another workspace',async()=>{
    await page.evaluate(()=>{window.__hold=true;window.__mask.toggle('new-column');});
    await page.waitForFunction(()=>window.__pending.length===1);
    await page.evaluate(()=>{window.__workspace('b');window.__releaseAll();});
    await page.waitForFunction(()=>window.__pending.length===0);
    assert.deepEqual(await page.evaluate(()=>window.__events),[]);
    assert.deepEqual(await page.evaluate(()=>window.__toasts),[]);
  });
  await check('an old connection list cannot replace the new workspace list',async()=>{
    await page.waitForFunction(()=>window.__list.status==='ready');
    await page.evaluate(()=>{window.__holdList=true;window.__list.refresh();});
    await page.waitForFunction(()=>window.__listPending?.length===1);
    await page.evaluate(()=>{window.__workspace('b');});
    await page.waitForFunction(()=>window.__listPending.length===2);
    assert.deepEqual(await page.evaluate(()=>window.__list.connections),[]);
    await page.evaluate(()=>window.__listPending[1]());
    await page.waitForFunction(()=>window.__list.connections[0]?.name==='Connection b');
    await page.evaluate(()=>window.__listPending[0]());
    assert.equal(await page.evaluate(()=>window.__list.connections[0]?.name),'Connection b');
  });
  await check('same-project refresh ignores an older result arriving last',async()=>{
    await page.waitForFunction(()=>window.__list.status==='ready');
    await page.evaluate(()=>{window.__holdList=true;window.__list.refresh();});
    await page.waitForFunction(()=>window.__listPending?.length===1);
    await page.evaluate(()=>{window.__connections[0].name='Latest';window.__list.refresh();});
    await page.waitForFunction(()=>window.__listPending.length===2);
    await page.evaluate(()=>window.__listPending[1]());
    await page.waitForFunction(()=>window.__list.connections[0]?.name==='Latest');
    await page.evaluate(()=>window.__listPending[0]());
    assert.equal(await page.evaluate(()=>window.__list.connections[0]?.name),'Latest');
  });
  await check('a failed load hides old connections and can be retried',async()=>{
    await page.waitForFunction(()=>window.__list.status==='ready');
    await page.evaluate(()=>{window.__failList=true;window.__workspace('b');});
    await page.waitForFunction(()=>window.__list.status==='error');
    assert.deepEqual(await page.evaluate(()=>window.__list.connections),[]);
    await page.evaluate(()=>{window.__failList=false;window.__list.refresh();});
    await page.waitForFunction(()=>window.__list.status==='ready');
    assert.equal(await page.evaluate(()=>window.__list.connections[0]?.name),'Connection b');
  });
  assert.deepEqual(errors,[]);
  console.log(`${passed} vault workspace scenarios passed (Chromium, simulated IPC).`);
} finally {await browser.close();}
