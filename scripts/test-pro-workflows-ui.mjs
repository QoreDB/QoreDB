// SPDX-License-Identifier: BUSL-1.1

import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';

const { chromium } = await import(process.env.QOREDB_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.QOREDB_CHROMIUM_EXECUTABLE,
});
const page = await browser.newPage({ viewport: { width: 1000, height: 750 } });
const errors = [];
const promptRequests = [];
page.on('pageerror', error => errors.push(error.message));
page.on('request', request => {
  if (request.url().includes('/QueryVariablesPrompt.tsx')) promptRequests.push(request.url());
});
await page.route('**/src/providers/LicenseProvider.tsx*', route => route.fulfill({
  contentType: 'application/javascript',
  body: 'export const useLicense = () => ({isFeatureEnabled:()=>true});',
}));
await page.addInitScript(() => {
  window.$RefreshReg$ = () => {};
  window.$RefreshSig$ = () => type => type;
  localStorage.setItem('qoredb_query_library_v1', JSON.stringify({
    folders: [],
    items: [{
      id: 'synthetic-query', title: 'Synthetic variables',
      query: 'SELECT {{text}}, $amount', tags: [], isFavorite: false,
      createdAt: 1, updatedAt: 1,
      variables: {
        text: { name: 'text', type: 'text', defaultValue: '$amount {{amount}}' },
        amount: { name: 'amount', type: 'number', defaultValue: '9007199254740993' },
      },
    }],
  }));
});
const base = process.env.QOREDB_UI_BASE_URL ?? 'http://127.0.0.1:1430';
try {
  await page.goto(new URL('/scripts/fixtures/pro-workflows/index.html?lang=en', base).href);
  await page.getByText('Incomplete comparison', { exact: true }).waitFor();
  assert.equal(JSON.parse(await page.locator('#stats').textContent()).total, 2);
  await page.getByText('Duplicate keys: remaining rows are paired in source order.', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Toggle rows' }).click();
  await page.getByText('No rows match this filter', { exact: true }).waitFor();
  assert.equal(await page.getByText('No differences found', { exact: true }).count(), 0);
  console.log('PASS duplicate rows and ambiguity warning; an empty filter does not claim equality');

  await page.getByRole('button', { name: 'truncated', exact: true }).click();
  await page.getByText('Only the loaded rows are compared.', { exact: true }).waitFor();
  await page.getByText('No differences observed in the compared data', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'equal', exact: true }).click();
  await page.getByText('No differences found', { exact: true }).waitFor();
  assert.equal(await page.getByText('Incomplete comparison', { exact: true }).count(), 0);
  console.log('PASS incomplete and complete empty results have distinct messages');

  assert.equal(promptRequests.length, 0);
  await page.getByRole('button', { name: 'Open library' }).click();
  await page.getByText('Synthetic variables', { exact: true }).waitFor();
  assert.equal(promptRequests.length, 0);
  const useQuery = page.getByRole('button', { name: 'Use query', exact: true });
  await useQuery.focus();
  await useQuery.press('Enter');
  await page.locator('#qv-text').waitFor();
  assert.equal(await page.locator('#qv-text').evaluate(element => document.activeElement === element), true);
  await page.locator('#qv-text').press('Tab');
  assert.equal(await page.locator('#qv-amount').evaluate(element => document.activeElement === element), true);
  await page.locator('#qv-amount').press('Shift+Tab');
  assert.equal(await page.locator('#qv-text').evaluate(element => document.activeElement === element), true);
  assert.equal(promptRequests.length, 1);
  assert.equal(await page.locator('#qv-amount').inputValue(), '9007199254740993');
  await page.locator('#qv-text').press('Escape');
  await page.getByRole('dialog').waitFor({ state: 'detached' });
  assert.equal(await useQuery.evaluate(element => document.activeElement === element), true);
  await page.waitForFunction(() => getComputedStyle(document.activeElement.parentElement).opacity === '1');
  assert.equal(await useQuery.evaluate(element => getComputedStyle(element.parentElement).opacity), '1');
  await useQuery.press('Enter');
  await page.locator('#qv-amount').waitFor();
  await page.locator('#qv-amount').press('Enter');
  await page.waitForFunction(() => document.querySelector('#selected-query').textContent === "SELECT '$amount {{amount}}', 9007199254740993");
  console.log('PASS variable dialog loads only when requested and submits exact, single-pass SQL');
  console.log('PASS keyboard opening, Escape restores visible focus, and Enter submits');

  await page.getByRole('button', { name: 'Open library' }).click();
  await page.getByRole('button', { name: 'Use query', exact: true }).click();
  await page.locator('#qv-amount').fill('0.12345678901234567890');
  await page.locator('#qv-amount').press('Enter');
  await page.waitForFunction(() => document.querySelector('#selected-query').textContent === "SELECT '$amount {{amount}}', 0.12345678901234567890");
  console.log('PASS keyboard submission accepts decimals and preserves every digit');

  await page.getByRole('button', { name: 'retry', exact: true }).click();
  await page.getByRole('button', { name: 'Retry', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#retry-result').textContent === 'fixture');
  console.log('PASS a failed source has an explicit retry action for its connection');

  await page.goto(new URL('/scripts/fixtures/pro-workflows/index.html?lang=fr', base).href);
  await page.getByText('Comparaison incomplète', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Toggle rows' }).click();
  await page.getByText('Aucune ligne pour ce filtre', { exact: true }).waitFor();
  await page.setViewportSize({ width: 600, height: 750 });
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.evaluate(() => document.documentElement.classList.add('dark'));
  await mkdir('.perf', { recursive: true });
  await page.screenshot({ path: '.perf/pro-workflows-ui.png' });
  assert.deepEqual(errors, []);
  console.log('PASS French warnings in a narrow dark view; no browser errors');
} finally {
  await browser.close();
}
