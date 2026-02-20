// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { highlightMatch } from './highlightMatch';

interface SettingsCardProps {
  id?: string;
  title: string;
  description?: string;
  children: ReactNode;
  isModified?: boolean;
  searchQuery?: string;
}

export function SettingsCard({
  id,
  title,
  description,
  children,
  isModified,
  searchQuery,
}: SettingsCardProps) {
  const { t } = useTranslation();
  const displayTitle = searchQuery ? highlightMatch(title, searchQuery) : title;
  const displayDescription =
    description && searchQuery ? highlightMatch(description, searchQuery) : description;

  return (
    <div id={id} className="py-4">
      <div className="flex items-center gap-2 mb-1">
        <h3 className="text-sm font-medium text-foreground">{displayTitle}</h3>
        {isModified && (
          <span className="px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider bg-primary/15 text-primary rounded">
            {t('settings.modified')}
          </span>
        )}
      </div>
      {displayDescription ? (
        <p className="text-xs text-muted-foreground mb-3">{displayDescription}</p>
      ) : null}
      <div>{children}</div>
    </div>
  );
}
