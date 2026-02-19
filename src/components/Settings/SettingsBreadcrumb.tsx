// SPDX-License-Identifier: Apache-2.0

import { ChevronRight } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { getSectionById, type SettingsSectionId } from './settingsConfig';

interface SettingsBreadcrumbProps {
  currentSection: SettingsSectionId;
  onNavigateHome: () => void;
}

export function SettingsBreadcrumb({ currentSection, onNavigateHome }: SettingsBreadcrumbProps) {
  const { t } = useTranslation();
  const section = getSectionById(currentSection);

  return (
    <nav className="flex items-center gap-1 text-sm text-muted-foreground">
      <button onClick={onNavigateHome} className="hover:text-foreground transition-colors">
        {t('settings.title')}
      </button>
      {section && (
        <>
          <ChevronRight size={14} className="shrink-0" />
          <span className="text-foreground font-medium">{t(section.labelKey)}</span>
        </>
      )}
    </nav>
  );
}
