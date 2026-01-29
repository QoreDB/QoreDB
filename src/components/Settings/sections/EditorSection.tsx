import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { SettingsCard } from '../SettingsCard';
import { getSandboxPreferences, setSandboxPreferences } from '@/lib/sandboxStore';
import { SandboxDeleteDisplay } from '@/lib/sandboxTypes';

interface EditorSectionProps {
  searchQuery?: string;
}

// Default values for detecting modifications
const DEFAULTS = {
  confirmOnDiscard: true,
  autoCollapsePanel: false,
  deleteDisplay: 'strikethrough' as SandboxDeleteDisplay,
  panelPageSize: 50,
};

export function EditorSection({ searchQuery }: EditorSectionProps) {
  const { t } = useTranslation();
  const [sandboxPrefs, setSandboxPrefs] = useState(getSandboxPreferences());
  const [panelPageSizeInput, setPanelPageSizeInput] = useState(
    String(getSandboxPreferences().panelPageSize)
  );

  function updateSandboxPrefs(next: Partial<typeof sandboxPrefs>) {
    setSandboxPreferences(next);
    const updated = getSandboxPreferences();
    setSandboxPrefs(updated);
    setPanelPageSizeInput(String(updated.panelPageSize));
  }

  const isModified =
    sandboxPrefs.confirmOnDiscard !== DEFAULTS.confirmOnDiscard ||
    sandboxPrefs.autoCollapsePanel !== DEFAULTS.autoCollapsePanel ||
    sandboxPrefs.deleteDisplay !== DEFAULTS.deleteDisplay ||
    sandboxPrefs.panelPageSize !== DEFAULTS.panelPageSize;

  return (
    <SettingsCard
      id="sandbox"
      title={t('settings.sandbox.title')}
      description={t('settings.sandbox.description')}
      isModified={isModified}
      searchQuery={searchQuery}
    >
      <div className="space-y-3">
        <label className="flex items-start gap-2.5 text-sm cursor-pointer">
          <Checkbox
            checked={sandboxPrefs.confirmOnDiscard}
            onCheckedChange={checked =>
              updateSandboxPrefs({ confirmOnDiscard: !!checked })
            }
            className="mt-0.5"
          />
          <span>
            <span className="font-medium text-foreground">{t('settings.sandbox.confirmDiscard')}</span>
            <span className="block text-xs text-muted-foreground mt-0.5">
              {t('settings.sandbox.confirmDiscardDescription')}
            </span>
          </span>
        </label>

        <label className="flex items-start gap-2.5 text-sm cursor-pointer">
          <Checkbox
            checked={sandboxPrefs.autoCollapsePanel}
            onCheckedChange={checked =>
              updateSandboxPrefs({ autoCollapsePanel: !!checked })
            }
            className="mt-0.5"
          />
          <span>
            <span className="font-medium text-foreground">{t('settings.sandbox.autoCollapse')}</span>
            <span className="block text-xs text-muted-foreground mt-0.5">
              {t('settings.sandbox.autoCollapseDescription')}
            </span>
          </span>
        </label>

        <div className="flex items-center gap-3 pt-2">
          <span className="text-sm text-foreground">
            {t('settings.sandbox.deleteDisplay')}
          </span>
          <Select
            value={sandboxPrefs.deleteDisplay}
            onValueChange={(value: SandboxDeleteDisplay) =>
              updateSandboxPrefs({ deleteDisplay: value })
            }
          >
            <SelectTrigger className="w-48 h-8 text-sm">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="strikethrough">
                {t('settings.sandbox.deleteDisplayStrikethrough')}
              </SelectItem>
              <SelectItem value="hidden">
                {t('settings.sandbox.deleteDisplayHidden')}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="flex items-center gap-3 pt-1">
          <span className="text-sm text-foreground">
            {t('settings.sandbox.panelPageSize')}
          </span>
          <Input
            type="number"
            min={20}
            step={10}
            className="w-20 h-8 text-sm"
            value={panelPageSizeInput}
            onChange={event => setPanelPageSizeInput(event.target.value)}
            onBlur={() => {
              const parsed = Number(panelPageSizeInput);
              if (Number.isFinite(parsed) && parsed >= 20) {
                updateSandboxPrefs({ panelPageSize: Math.floor(parsed) });
              } else {
                setPanelPageSizeInput(String(sandboxPrefs.panelPageSize));
              }
            }}
          />
          <span className="text-xs text-muted-foreground">
            {t('settings.sandbox.panelPageSizeDescription')}
          </span>
        </div>
      </div>
    </SettingsCard>
  );
}
