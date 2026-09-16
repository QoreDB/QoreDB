// SPDX-License-Identifier: BUSL-1.1

import { EyeOff, Plus, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { LicenseBadge } from '@/components/License/LicenseBadge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import type { ConnectionMasking, MaskingRule, MaskMode } from '@/lib/tauri';
import { useLicense } from '@/providers/LicenseProvider';

const MODES: MaskMode[] = ['hidden', 'partial', 'hash'];

interface MaskingSectionProps {
  masking: ConnectionMasking;
  onChange: (next: ConnectionMasking) => void;
}

/** Without Pro, rules can still be removed: a lapsed licence never locks masks in place. */
export function MaskingSection({ masking, onChange }: MaskingSectionProps) {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const canEdit = isFeatureEnabled('column_masking');

  function updateRule(index: number, patch: Partial<MaskingRule>) {
    onChange({
      ...masking,
      rules: masking.rules.map((rule, i) => (i === index ? { ...rule, ...patch } : rule)),
    });
  }

  return (
    <div className="rounded-md border border-border bg-background p-4 space-y-4">
      <div className="space-y-0.5">
        <Label className="flex items-center gap-2">
          <EyeOff size={14} className="text-muted-foreground" />
          {t('connection.masking.title')}
          {!canEdit && <LicenseBadge tier="pro" />}
        </Label>
        <p className="text-xs text-muted-foreground">{t('connection.masking.description')}</p>
      </div>

      <div className="flex items-center justify-between gap-3 rounded-md border border-border px-3 py-2">
        <div className="min-w-0 space-y-0.5">
          <span className="text-sm">{t('connection.masking.detected')}</span>
          <p className="text-xs text-muted-foreground">{t('connection.masking.detectedHint')}</p>
        </div>
        <Switch
          checked={masking.mask_detected_columns}
          disabled={!canEdit && !masking.mask_detected_columns}
          onCheckedChange={checked => onChange({ ...masking, mask_detected_columns: checked })}
        />
      </div>

      {masking.rules.length === 0 ? (
        <p className="text-xs text-muted-foreground">{t('connection.masking.empty')}</p>
      ) : (
        <div className="space-y-2">
          <div className="grid grid-cols-[1fr_1fr_7rem_2.25rem] gap-2 text-xs text-muted-foreground">
            <span>{t('connection.masking.table')}</span>
            <span>{t('connection.masking.column')}</span>
            <span>{t('connection.masking.mode')}</span>
            <span />
          </div>
          {masking.rules.map((rule, index) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: rules have no identity and every field is controlled
            <div key={index} className="grid grid-cols-[1fr_1fr_7rem_2.25rem] gap-2">
              <Input
                value={rule.table}
                placeholder={t('connection.masking.anyTable')}
                disabled={!canEdit}
                onChange={e => updateRule(index, { table: e.target.value })}
                spellCheck={false}
              />
              <Input
                value={rule.column}
                disabled={!canEdit}
                onChange={e => updateRule(index, { column: e.target.value })}
                spellCheck={false}
              />
              <Select
                value={rule.mode}
                disabled={!canEdit}
                onValueChange={value => updateRule(index, { mode: value as MaskMode })}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {MODES.map(mode => (
                    <SelectItem key={mode} value={mode}>
                      {t(`connection.masking.modes.${mode}`)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                aria-label={t('connection.masking.removeRule')}
                title={t('connection.masking.removeRule')}
                onClick={() =>
                  onChange({ ...masking, rules: masking.rules.filter((_, i) => i !== index) })
                }
              >
                <Trash2 size={14} />
              </Button>
            </div>
          ))}
        </div>
      )}

      <div className="flex items-center justify-between gap-3">
        {!canEdit && (
          <p className="text-xs text-muted-foreground">{t('connection.masking.proHint')}</p>
        )}
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="ml-auto gap-1.5"
          disabled={!canEdit}
          onClick={() =>
            onChange({
              ...masking,
              rules: [...masking.rules, { table: '', column: '', mode: 'partial' }],
            })
          }
        >
          <Plus size={14} />
          {t('connection.masking.addRule')}
        </Button>
      </div>
    </div>
  );
}
