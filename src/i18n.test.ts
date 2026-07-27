// SPDX-License-Identifier: Apache-2.0

import { afterAll, describe, expect, it } from 'vitest';
import i18n, { i18nReady } from './i18n';

const LOCALE_EXPECTATIONS = [
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
  afterAll(async () => {
    await i18n.changeLanguage('en');
  });

  it.each(LOCALE_EXPECTATIONS)('loads %s on demand', async (language, expectedCancel) => {
    await i18nReady;
    if (i18n.hasResourceBundle(language, 'translation')) {
      i18n.removeResourceBundle(language, 'translation');
    }

    await i18n.changeLanguage(language);

    expect(i18n.hasResourceBundle(language, 'translation')).toBe(true);
    expect(i18n.t('common.cancel')).toBe(expectedCancel);
  });
});
