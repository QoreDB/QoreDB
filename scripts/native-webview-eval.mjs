// SPDX-License-Identifier: Apache-2.0
// Linux WebKitGTK inspector client for an explicitly isolated QoreDB process.
// This evaluates the real WebView: it does not replace IPC, dialogs or file IO.
import { readFileSync } from 'node:fs';

const endpoint = process.env.QOREDB_INSPECTOR_URL;
if (!endpoint) throw new Error('Set QOREDB_INSPECTOR_URL to the isolated app inspector');
const page = await (await fetch(endpoint)).text();
const target = page.match(/\/socket\/\d+\/\d+\/WebPage/);
if (!target) throw new Error('No inspectable native WebView');
const socket = new WebSocket(new URL(target[0], endpoint.replace('http:', 'ws:')));
let sequence = 0;
let targetId;
const pending = new Map();
socket.addEventListener('message', ({ data }) => {
  let message = JSON.parse(data);
  if (message.method === 'Target.targetCreated' && message.params.targetInfo.type === 'page') targetId = message.params.targetInfo.targetId;
  if (message.method === 'Target.dispatchMessageFromTarget') message = JSON.parse(message.params.message);
  const request = pending.get(message.id);
  if (!request) return;
  pending.delete(message.id);
  if (message.error) request.reject(new Error(JSON.stringify(message.error)));
  else request.resolve(message.result);
});
function send(method, params) {
  return new Promise((resolve, reject) => {
    const id = ++sequence;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({id: ++sequence, method: 'Target.sendMessageToTarget', params: { targetId, message: JSON.stringify({ id, method, params }) }}));
  });
}
await new Promise((resolve, reject) => {
  socket.addEventListener('open', resolve, { once: true });
  socket.addEventListener('error', reject, { once: true });
});
for (let attempt = 0; !targetId && attempt < 50; attempt++) await new Promise(resolve => setTimeout(resolve, 20));
if (!targetId) throw new Error('No WebKit page target');
const timer = setTimeout(() => {
  console.error('Native script timed out');
  process.exit(1);
}, Number(process.env.QOREDB_NATIVE_TIMEOUT_MS || 30000));
try {
  const source = readFileSync(0, 'utf8');
  const slot = `__qualification_${Date.now()}`;
  const started = await send('Runtime.evaluate', {
    expression: `window[${JSON.stringify(slot)}] = null; (async () => {${source}\n})().then(value => {window[${JSON.stringify(slot)}] = {ok:true,value}}, error => {window[${JSON.stringify(slot)}] = {ok:false,error:String(error),stack:error?.stack}});`,
    returnByValue: false,
  });
  if (started.wasThrown) throw new Error(JSON.stringify(started));
  for (;;) {
    const result = await send('Runtime.evaluate', {
      expression: `window[${JSON.stringify(slot)}]`, returnByValue: true,
    });
    if (result.result?.value) {
      const outcome = result.result.value;
      if (!outcome.ok) throw new Error(JSON.stringify(outcome));
      console.log(JSON.stringify(outcome.value, null, 2));
      await send('Runtime.evaluate', { expression: `delete window[${JSON.stringify(slot)}]` });
      break;
    }
    await new Promise(resolve => setTimeout(resolve, 100));
  }
} finally {
  clearTimeout(timer);
  socket.close();
}
