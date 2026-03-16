// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';

export function SkipLink() {
  const { t } = useTranslation();

  return (
    <a
      href="#main-content"
      className="sr-only focus:not-sr-only focus:absolute focus:z-[100] focus:top-2 focus:left-2 focus:px-4 focus:py-2 focus:bg-background focus:border focus:border-border focus:rounded-md focus:text-sm focus:font-medium focus:shadow-lg"
    >
      {t('a11y.skipToContent')}
    </a>
  );
}
