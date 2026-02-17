// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import { Search, X } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';

interface SettingsSearchProps {
  value: string;
  onChange: (value: string) => void;
}

export function SettingsSearch({ value, onChange }: SettingsSearchProps) {
  const { t } = useTranslation();

  return (
    <div className="relative w-full max-w-sm">
      <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground pointer-events-none" />
      <Input
        type="text"
        placeholder={t('settings.search.placeholder')}
        value={value}
        onChange={e => onChange(e.target.value)}
        className="pl-9 pr-9"
      />
      {value && (
        <Button
          variant="ghost"
          size="icon"
          className="absolute right-1 top-1/2 -translate-y-1/2 h-7 w-7"
          onClick={() => onChange('')}
        >
          <X size={14} />
        </Button>
      )}
    </div>
  );
}
