// SPDX-License-Identifier: Apache-2.0

import { createInstance } from 'i18next';
import { describe, expect, it } from 'vitest';
import i18n, { i18nReady, localeBackend } from './i18n';

const LOCALE_EXPECTATIONS = [
  ['en', 'Cancel'],
  ['fr', 'Annuler'],
  ['es', 'Cancelar'],
  ['de', 'Abbrechen'],
  ['pt-BR', 'Cancelar'],
  ['zh-CN', '取消'],
  ['ja', 'キャンセル'],
  ['ko', '취소'],
  ['ru', 'Отмена'],
] as const;

describe('i18n locale loading', () => {
  // On its own instance rather than the app's: the shared one detects its
  // language from the machine, so asserting on it makes the result depend on
  // where the suite runs.
  it.each(LOCALE_EXPECTATIONS)('loads %s on demand', async (language, expectedCancel) => {
    const instance = createInstance();
    await instance.use(localeBackend).init({
      lng: language,
      // Without this a missing bundle would answer in English and pass.
      fallbackLng: false,
      interpolation: { escapeValue: false },
    });

    expect(instance.hasResourceBundle(language, 'translation')).toBe(true);
    expect(instance.t('common.cancel')).toBe(expectedCancel);
  });

  it('reports an unknown locale instead of loading something else', async () => {
    const instance = createInstance();
    await instance.use(localeBackend).init({
      lng: 'xx',
      fallbackLng: false,
      interpolation: { escapeValue: false },
    });

    expect(instance.hasResourceBundle('xx', 'translation')).toBe(false);
  });

  it('bundles English into the app instance', async () => {
    await i18nReady;
    expect(i18n.hasResourceBundle('en', 'translation')).toBe(true);
  });
});
