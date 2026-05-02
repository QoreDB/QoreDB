// SPDX-License-Identifier: Apache-2.0

import { FileEdit, Sparkles } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { BULK_EDIT_CORE_LIMIT } from '@/lib/bulkEdit';

interface BulkEditButtonProps {
  selectedCount: number;
  disabled: boolean;
  requiresPro: boolean;
  readOnly: boolean;
  mutationsSupported: boolean;
  onClick: () => void;
}

export function BulkEditButton({
  selectedCount,
  disabled,
  requiresPro,
  readOnly,
  mutationsSupported,
  onClick,
}: BulkEditButtonProps) {
  const { t } = useTranslation();

  const title = readOnly
    ? t('environment.blocked')
    : !mutationsSupported
      ? t('grid.mutationsNotSupported')
      : requiresPro
        ? t('bulkEdit.proRequiredTooltip', { limit: BULK_EDIT_CORE_LIMIT })
        : t('bulkEdit.title');

  return (
    <Button
      variant="outline"
      size="sm"
      className="h-6 px-2 text-xs"
      onClick={onClick}
      disabled={disabled}
      title={title}
      aria-label={t('bulkEdit.title')}
    >
      {requiresPro ? (
        <Sparkles size={12} className="mr-1 text-accent" />
      ) : (
        <FileEdit size={12} className="mr-1" />
      )}
      {t('bulkEdit.button', { count: selectedCount })}
    </Button>
  );
}
