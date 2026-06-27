// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import { DangerConfirmDialog } from '@/components/Guard/DangerConfirmDialog';
import { resolveConfirm, useConfirmState } from '@/lib/stores/confirmStore';

export function ConfirmHost() {
  const { t } = useTranslation();
  const { open, options } = useConfirmState();

  return (
    <DangerConfirmDialog
      open={open}
      title={options?.title ?? t('common.confirm')}
      description={options?.description ?? ''}
      confirmLabel={options?.confirmLabel ?? t('common.confirm')}
      confirmationLabel={options?.confirmationLabel}
      warningInfo={options?.warningInfo}
      onConfirm={() => resolveConfirm(true)}
      onOpenChange={isOpen => {
        if (!isOpen) resolveConfirm(false);
      }}
    />
  );
}
