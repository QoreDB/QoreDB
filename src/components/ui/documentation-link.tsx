// SPDX-License-Identifier: Apache-2.0

import { ExternalLink } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { getDocsUrl } from '@/lib/externalLinks';
import { openExternal } from '@/lib/transport';
import { cn } from '@/lib/utils';
import { Button } from './button';

interface DocumentationLinkProps {
  path?: string;
  label?: string;
  className?: string;
}

export function DocumentationLink({ path, label, className }: DocumentationLinkProps) {
  const { t } = useTranslation();

  return (
    <Button
      type="button"
      variant="link"
      size="sm"
      className={cn('h-auto gap-1 px-0 text-xs', className)}
      onClick={() => void openExternal(getDocsUrl(path))}
    >
      {label ?? t('common.viewDocumentation')}
      <ExternalLink size={12} aria-hidden />
    </Button>
  );
}
