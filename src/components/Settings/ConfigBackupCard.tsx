import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { save, open as openDialog } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { toast } from 'sonner';
import { Download, Upload } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { SettingsCard } from './SettingsCard';
import { applyConfigBackupV1, buildConfigBackupV1, isConfigBackupV1 } from '@/lib/configBackup';
import type { DiagnosticsSettings } from '@/lib/diagnosticsSettings';
import type { SafetyPolicy } from '@/lib/tauri';
import { useTheme } from '@/hooks/useTheme';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';

interface ConfigBackupCardProps {
  policy: SafetyPolicy | null;
  onApplyDiagnostics: (next: DiagnosticsSettings) => void;
  onApplyPolicy: (next: SafetyPolicy) => Promise<void>;
  onApplyAnalyticsEnabled: (enabled: boolean) => void;
}

const MAX_CONFIG_BACKUP_BYTES = 2_000_000;

export function ConfigBackupCard({
  policy,
  onApplyDiagnostics,
  onApplyPolicy,
  onApplyAnalyticsEnabled,
}: ConfigBackupCardProps) {
  const { t, i18n } = useTranslation();
  const { setTheme } = useTheme();
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);

  async function handleExport() {
    setExporting(true);
    try {
      const payload = buildConfigBackupV1({ safetyPolicy: policy ?? undefined });
      const filePath = await save({
        defaultPath: 'qoredb-config-backup.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath) return;

      await writeTextFile(filePath, JSON.stringify(payload, null, 2));
      revealItemInDir(filePath).catch(() => undefined);

      const name = filePath.split(/[\\/]/).pop() || filePath;
      toast.success(t('settings.configBackupExportSuccess', { name }), {
        description: filePath,
      });
    } catch (err) {
      toast.error(t('settings.configBackupExportError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setExporting(false);
    }
  }

  async function handleImport() {
    if (!confirm(t('settings.configBackupImportConfirm'))) return;

    setImporting(true);
    try {
      const filePath = await openDialog({
        multiple: false,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath || Array.isArray(filePath)) return;

      const raw = await readTextFile(filePath);
      if (raw.length > MAX_CONFIG_BACKUP_BYTES) {
        throw new Error(t('settings.configBackupTooLarge'));
      }

      const parsed: unknown = JSON.parse(raw);
      if (!isConfigBackupV1(parsed)) {
        throw new Error(t('settings.configBackupInvalid'));
      }

      const applied = applyConfigBackupV1(parsed);

      if (applied.theme) setTheme(applied.theme);
      if (applied.language) await i18n.changeLanguage(applied.language);
      if (applied.diagnostics) onApplyDiagnostics(applied.diagnostics);
      if (applied.analyticsEnabled !== undefined) {
        const enabled = !!applied.analyticsEnabled;
        onApplyAnalyticsEnabled(enabled);
        AnalyticsService.setAnalyticsEnabled(enabled);
      }
      if (applied.safetyPolicy) await onApplyPolicy(applied.safetyPolicy);

      const name = filePath.split(/[\\/]/).pop() || filePath;
      toast.success(t('settings.configBackupImportSuccess', { name }));
    } catch (err) {
      toast.error(t('settings.configBackupImportError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setImporting(false);
    }
  }

  return (
    <SettingsCard
      title={t('settings.configBackup')}
      description={t('settings.configBackupDescription')}
    >
      <div className="flex flex-wrap gap-2">
        <Button variant="outline" onClick={handleExport} disabled={exporting}>
          <Download size={16} className="mr-2" />
          {t('settings.configBackupExport')}
        </Button>
        <Button variant="outline" onClick={handleImport} disabled={importing}>
          <Upload size={16} className="mr-2" />
          {t('settings.configBackupImport')}
        </Button>
      </div>
    </SettingsCard>
  );
}
