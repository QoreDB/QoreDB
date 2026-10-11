// SPDX-License-Identifier: Apache-2.0
// Requires the synthetic large_grid table open in the real native WebView.
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(process.argv[2]);
const manifest = JSON.parse(readFileSync(resolve(root, 'fixture-manifest.json'), 'utf8'));
if (manifest.baseline !== '252e0b267b6d67585b6916bce2ce29f516685fd8') throw Error('Wrong fixture');

async function qualify(root) {
  const connections = await window.__TAURI_INTERNALS__.invoke('list_saved_connections', { projectId: 'default' });
  if (!connections.length || connections.some(c => c.driver !== 'sqlite' || c.host !== root + '/fixture.sqlite')) {
    throw Error('Refusing a non-fixture connection');
  }
  if (!document.body.innerText.includes('large_grid')) throw Error('Open large_grid first');
  const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
  async function until(check) {
    const deadline = performance.now() + 10000;
    while (!check()) {
      if (performance.now() > deadline) throw Error('Native UI condition timed out');
      await pause(10);
    }
  }
  const count = () => Number(document.body.innerText.match(/(\d+) rows loaded/)?.[1] ?? 0);
  const scroller = [...document.querySelectorAll('[data-datagrid] div')].find(
    x => x.scrollHeight > x.clientHeight + 100 && getComputedStyle(x).overflowY === 'auto'
  );
  if (!scroller) throw Error('Missing native grid scroller');
  const pages = [];
  while (count() < 500) {
    const before = count();
    const start = performance.now();
    scroller.scrollTop = scroller.scrollHeight;
    scroller.dispatchEvent(new Event('scroll'));
    await until(() => count() > before);
    pages.push({ rows: count(), durationMs: performance.now() - start });
  }
  scroller.scrollTop = 7000;
  await pause(100);
  const visible = [...document.querySelectorAll('button[aria-label="Edit cell: label"]')].find(b => {
    const r = b.getBoundingClientRect(); const s = scroller.getBoundingClientRect();
    return r.top >= s.top && r.bottom <= s.bottom;
  });
  if (!visible) throw Error('No visible editable row');
  const row = visible.closest('tr');
  const id = Number(row.querySelector('[aria-label="Edit cell: id"]').textContent);
  const checkbox = row.querySelector('[role="checkbox"]');
  if (checkbox.getAttribute('aria-checked') !== 'true') checkbox.click();
  const scrollTop = scroller.scrollTop;
  const loadedRows = count();
  const edits = [];
  const button = () => [...document.querySelectorAll('[aria-label="Edit cell: id"]')]
    .find(b => Number(b.textContent) === id)?.closest('tr')?.querySelector('[aria-label="Edit cell: label"]');
  for (let i = 0; i < 30; i++) {
    const cell = button();
    cell.focus({ preventScroll: true });
    cell.dispatchEvent(new KeyboardEvent('keydown', { key: 'F2', bubbles: true, cancelable: true }));
    await until(() => document.querySelector('input[aria-label="Edit cell"]'));
    const input = document.querySelector('input[aria-label="Edit cell"]');
    const value = `Native edit ${String(i).padStart(2, '0')}`;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true, cancelable: true }));
    await pause(0);
    const start = performance.now();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    await until(() => button()?.textContent === value && !document.querySelector('input[aria-label="Edit cell"]'));
    await until(() => document.activeElement === button());
    if (scroller.scrollTop !== scrollTop || count() !== loadedRows || button().closest('tr').querySelector('[role="checkbox"]').getAttribute('aria-checked') !== 'true') {
      throw Error('Edit lost position, loaded pages or selection');
    }
    edits.push(performance.now() - start);
  }
  button().dispatchEvent(new KeyboardEvent('keydown', { key: 'F2', bubbles: true, cancelable: true }));
  await until(() => document.querySelector('input[aria-label="Edit cell"]'));
  document.querySelector('input[aria-label="Edit cell"]').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
  await until(() => document.activeElement === button());
  await pause(150);
  if (!scroller.isConnected || !button()) throw Error('Escape closed the table');
  if (button().textContent !== 'Native edit 29') throw Error('Escape changed the saved value');
  const sorted = [...edits].sort((a, b) => a - b);
  return { fixtureRows: 100000, payloadBytesPerRow: 1024, id, loadedRows, scrollTop, pages, editsMs: edits,
    medianMs: sorted[Math.ceil(sorted.length * .5) - 1], p95Ms: sorted[Math.ceil(sorted.length * .95) - 1],
    positionSelectionFocusPreserved: true, escapePreservedValue: true,
    limitation: 'DOM keyboard events in native WebKit; timings include 10 ms polling, no OS key dispatch or baseline comparison.' };
}

const result = spawnSync(process.execPath, [fileURLToPath(new URL('./native-webview-eval.mjs', import.meta.url))], {
  input: `return await (${qualify.toString()})(${JSON.stringify(root)});`, encoding: 'utf8',
  env: { ...process.env, QOREDB_NATIVE_TIMEOUT_MS: '120000' },
});
process.stdout.write(result.stdout || '');
process.stderr.write(result.stderr || '');
process.exit(result.status ?? 1);
