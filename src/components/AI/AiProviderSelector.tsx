// SPDX-License-Identifier: BUSL-1.1

import { useTranslation } from 'react-i18next';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { AI_PROVIDERS, type AiProvider } from '@/lib/ai';
import { AlertCircle } from 'lucide-react';

interface AiProviderSelectorProps {
  provider: AiProvider;
  onProviderChange: (provider: AiProvider) => void;
  providerHasKey?: Record<AiProvider, boolean>;
}

export function AiProviderSelector({
  provider,
  onProviderChange,
  providerHasKey,
}: AiProviderSelectorProps) {
  const { t } = useTranslation();

  return (
    <Select value={provider} onValueChange={v => onProviderChange(v as AiProvider)}>
      <SelectTrigger className="h-8 w-40 text-xs">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {AI_PROVIDERS.map(p => {
          const hasKey = providerHasKey ? providerHasKey[p.id] : true;
          return (
            <SelectItem key={p.id} value={p.id}>
              <span className="flex items-center gap-1.5">
                {p.label}
                {!p.requiresKey && (
                  <span className="text-[10px] px-1 py-0.5 rounded bg-muted text-muted-foreground">
                    {t('ai.ollamaLocal')}
                  </span>
                )}
                {p.requiresKey && !hasKey && <AlertCircle size={12} className="text-warning" />}
              </span>
            </SelectItem>
          );
        })}
      </SelectContent>
    </Select>
  );
}
