// SPDX-License-Identifier: Apache-2.0

import i18n, { type BackendModule, type ResourceKey } from 'i18next';
import LanguageDetector from 'i18next-browser-languagedetector';
import { initReactI18next } from 'react-i18next';
import en from './locales/en.json';

type TranslationModule = { default: ResourceKey };

const SUPPORTED_LANGUAGES = ['en', 'fr', 'es', 'de', 'pt-BR', 'zh-CN', 'ja', 'ko', 'ru'] as const;

const localeLoaders: Record<string, () => Promise<TranslationModule>> = {
  en: async () => ({ default: en }),
  fr: () => import('./locales/fr.json'),
  es: () => import('./locales/es.json'),
  de: () => import('./locales/de.json'),
  'pt-BR': () => import('./locales/pt-BR.json'),
  'zh-CN': () => import('./locales/zh-CN.json'),
  ja: () => import('./locales/ja.json'),
  ko: () => import('./locales/ko.json'),
  ru: () => import('./locales/ru.json'),
};

const localeBackend: BackendModule = {
  type: 'backend',
  init() {},
  read(language, namespace, callback) {
    const loader = namespace === 'translation' ? localeLoaders[language] : undefined;
    if (!loader) {
      callback(new Error(`Unsupported locale: ${language}/${namespace}`), false);
      return;
    }

    loader()
      .then(module => callback(null, module.default))
      .catch(error => callback(error instanceof Error ? error : new Error(String(error)), false));
  },
};

export const i18nReady = i18n
  .use(localeBackend)
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    supportedLngs: SUPPORTED_LANGUAGES,
    partialBundledLanguages: true,
    resources: {
      en: {
        translation: en,
      },
    },
    fallbackLng: 'en',
    interpolation: {
      escapeValue: false,
    },
    detection: {
      order: ['querystring', 'localStorage', 'navigator'],
      lookupQuerystring: 'lang',
      lookupLocalStorage: 'i18nextLng',
      caches: ['localStorage'],
    },
  });

export default i18n;
