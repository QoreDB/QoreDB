import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Settings, Moon, Sun, Monitor, ChevronDown, FlaskConical } from 'lucide-react';

import { useTheme } from '../../hooks/useTheme';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { SettingsCard } from './SettingsCard';
import { ConfigBackupCard } from './ConfigBackupCard';
import { ProjectTransferCard } from './ProjectTransferCard';

import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';

import { clearErrorLogs } from '@/lib/errorLog';
import { clearHistory } from '@/lib/history';
import {
  getDiagnosticsSettings,
  setDiagnosticsSettings,
  DiagnosticsSettings,
} from '@/lib/diagnosticsSettings';
import { getSafetyPolicy, setSafetyPolicy, SafetyPolicy } from '@/lib/tauri';
import { getSandboxPreferences, setSandboxPreferences } from '@/lib/sandboxStore';
import { SandboxDeleteDisplay } from '@/lib/sandboxTypes';

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const { theme, resolvedTheme, setTheme } = useTheme();
  const [diagnostics, setDiagnostics] = useState<DiagnosticsSettings>(getDiagnosticsSettings());
  const [analyticsEnabled, setAnalyticsEnabled] = useState<boolean>(
    AnalyticsService.isAnalyticsEnabled()
  );
  const [sandboxPrefs, setSandboxPrefs] = useState(getSandboxPreferences());
  const [panelPageSizeInput, setPanelPageSizeInput] = useState(
    String(getSandboxPreferences().panelPageSize)
  );

  const [policy, setPolicy] = useState<SafetyPolicy | null>(null);
  const [policyError, setPolicyError] = useState<string | null>(null);
  const [policySaving, setPolicySaving] = useState(false);

  const DEFAULT_PROJECT_ID = 'default';

  useEffect(() => {
    let active = true;
    getSafetyPolicy()
      .then(result => {
        if (!active) return;
        if (result.success && result.policy) {
          setPolicy(result.policy);
          setPolicyError(null);
        } else {
          setPolicyError(result.error || t('settings.safetyPolicyError'));
        }
      })
      .catch(() => {
        if (!active) return;
        setPolicyError(t('settings.safetyPolicyError'));
      });

    return () => {
      active = false;
    };
  }, [t]);

  function updateDiagnostics(next: DiagnosticsSettings) {
    setDiagnostics(next);
    setDiagnosticsSettings(next);
    if (!next.storeHistory) {
      clearHistory();
    }
    if (!next.storeErrorLogs) {
      clearErrorLogs();
    }
  }

  function updateSandboxPrefs(next: Partial<typeof sandboxPrefs>) {
    setSandboxPreferences(next);
    const updated = getSandboxPreferences();
    setSandboxPrefs(updated);
    setPanelPageSizeInput(String(updated.panelPageSize));
  }

  async function updatePolicy(next: SafetyPolicy) {
    setPolicy(next);
    setPolicySaving(true);
    setPolicyError(null);

    try {
      const result = await setSafetyPolicy(next);
      if (result.success && result.policy) {
        setPolicy(result.policy);
      } else {
        setPolicyError(result.error || t('settings.safetyPolicyError'));
      }
    } catch {
      setPolicyError(t('settings.safetyPolicyError'));
    } finally {
      setPolicySaving(false);
    }
  }

  return (
    <div className="flex flex-col h-full bg-background p-8 overflow-auto">
      <div className="max-w-2xl mx-auto w-full space-y-8">
        <div className="flex items-center gap-3 mb-8">
          <div className="p-3 rounded-lg bg-primary/10 text-primary">
            <Settings size={32} />
          </div>
          <div>
            <h1 className="text-3xl font-bold tracking-tight">{t('settings.title')}</h1>
          </div>
        </div>

        <div className="grid gap-6">
          <SettingsCard
            title={t('settings.language')}
            description={t('settings.languageDescription')}
          >
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" className="w-50 justify-between">
                  {i18n.language.startsWith('fr') ? 'Français' : 'English'}
                  <ChevronDown className="ml-2 h-4 w-4 opacity-50" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent className="w-50">
                <DropdownMenuItem onClick={() => i18n.changeLanguage('en')}>
                  English
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => i18n.changeLanguage('fr')}>
                  Français
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </SettingsCard>

          <SettingsCard title={t('settings.theme')} description={t('settings.themeDescription')}>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" className="w-50 justify-between">
                  <div className="flex items-center gap-2">
                    {theme === 'auto' ? (
                      <Monitor size={16} />
                    ) : theme === 'dark' ? (
                      <Moon size={16} />
                    ) : (
                      <Sun size={16} />
                    )}

                    {theme === 'auto'
                      ? `${t('settings.themeSystem')} (${resolvedTheme === 'dark' ? t('settings.themeDark') : t('settings.themeLight')})`
                      : theme === 'dark'
                        ? t('settings.themeDark')
                        : t('settings.themeLight')}
                  </div>
                  <ChevronDown className="ml-2 h-4 w-4 opacity-50" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent className="w-50">
                <DropdownMenuItem onClick={() => setTheme('auto')}>
                  <div className="flex items-center gap-2">
                    <Monitor size={16} />
                    {t('settings.themeSystem')}
                  </div>
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => setTheme('light')}>
                  <div className="flex items-center gap-2">
                    <Sun size={16} />
                    {t('settings.themeLight')}
                  </div>
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => setTheme('dark')}>
                  <div className="flex items-center gap-2">
                    <Moon size={16} />
                    {t('settings.themeDark')}
                  </div>
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </SettingsCard>

          <SettingsCard
            title={t('settings.diagnostics')}
            description={t('settings.diagnosticsDescription')}
          >
            <div className="space-y-4">
              <label className="flex items-start gap-3 text-sm">
                <Checkbox
                  checked={diagnostics.storeHistory}
                  onCheckedChange={checked =>
                    updateDiagnostics({
                      ...diagnostics,
                      storeHistory: !!checked,
                    })
                  }
                />
                <span>
                  <span className="font-medium">{t('settings.storeHistory')}</span>
                  <span className="block text-xs text-muted-foreground">
                    {t('settings.storeHistoryDescription')}
                  </span>
                </span>
              </label>

              <label className="flex items-start gap-3 text-sm">
                <Checkbox
                  checked={diagnostics.storeErrorLogs}
                  onCheckedChange={checked =>
                    updateDiagnostics({
                      ...diagnostics,
                      storeErrorLogs: !!checked,
                    })
                  }
                />
                <span>
                  <span className="font-medium">{t('settings.storeErrorLogs')}</span>
                  <span className="block text-xs text-muted-foreground">
                    {t('settings.storeErrorLogsDescription')}
                  </span>
                </span>
              </label>

              <label className="flex items-start gap-3 text-sm">
                <Checkbox
                  checked={analyticsEnabled}
                  onCheckedChange={checked => {
                    const enabled = !!checked;
                    setAnalyticsEnabled(enabled);
                    AnalyticsService.setAnalyticsEnabled(enabled);
                  }}
                />
                <span>
                  <span className="font-medium">{t('settings.analyticsEnabled')}</span>
                  <span className="block text-xs text-muted-foreground">
                    {t('settings.analyticsEnabledDescription')}
                  </span>
                </span>
              </label>
            </div>
          </SettingsCard>

          <SettingsCard
            title={t('settings.sandbox.title')}
            description={t('settings.sandbox.description')}
          >
            <div className="space-y-4">
              <label className="flex items-start gap-3 text-sm">
                <Checkbox
                  checked={sandboxPrefs.confirmOnDiscard}
                  onCheckedChange={checked =>
                    updateSandboxPrefs({ confirmOnDiscard: !!checked })
                  }
                />
                <span>
                  <span className="font-medium">{t('settings.sandbox.confirmDiscard')}</span>
                  <span className="block text-xs text-muted-foreground">
                    {t('settings.sandbox.confirmDiscardDescription')}
                  </span>
                </span>
              </label>

              <label className="flex items-start gap-3 text-sm">
                <Checkbox
                  checked={sandboxPrefs.autoCollapsePanel}
                  onCheckedChange={checked =>
                    updateSandboxPrefs({ autoCollapsePanel: !!checked })
                  }
                />
                <span>
                  <span className="font-medium">{t('settings.sandbox.autoCollapse')}</span>
                  <span className="block text-xs text-muted-foreground">
                    {t('settings.sandbox.autoCollapseDescription')}
                  </span>
                </span>
              </label>

              <div className="flex items-center gap-3 text-sm">
                <div className="flex items-center gap-2 text-muted-foreground">
                  <FlaskConical size={16} />
                  <span className="font-medium text-foreground">
                    {t('settings.sandbox.deleteDisplay')}
                  </span>
                </div>
                <Select
                  value={sandboxPrefs.deleteDisplay}
                  onValueChange={(value: SandboxDeleteDisplay) =>
                    updateSandboxPrefs({ deleteDisplay: value })
                  }
                >
                  <SelectTrigger className="w-56">
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

              <div className="flex items-center gap-3 text-sm">
                <span className="font-medium text-foreground">
                  {t('settings.sandbox.panelPageSize')}
                </span>
                <Input
                  type="number"
                  min={20}
                  step={10}
                  className="w-24"
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

          <SettingsCard
            title={t('settings.safetyPolicy')}
            description={t('settings.safetyPolicyDescription')}
          >
            <div className="space-y-4">
              <label className="flex items-start gap-3 text-sm">
                <Checkbox
                  checked={policy?.prod_require_confirmation ?? false}
                  disabled={!policy || policySaving}
                  onCheckedChange={checked =>
                    policy &&
                    updatePolicy({
                      ...policy,
                      prod_require_confirmation: !!checked,
                    })
                  }
                />
                <span>
                  <span className="font-medium">
                    {t('settings.safetyPolicyRequireConfirmation')}
                  </span>
                  <span className="block text-xs text-muted-foreground">
                    {t('settings.safetyPolicyRequireConfirmationDescription')}
                  </span>
                </span>
              </label>

              <label className="flex items-start gap-3 text-sm">
                <Checkbox
                  checked={policy?.prod_block_dangerous_sql ?? false}
                  disabled={!policy || policySaving}
                  onCheckedChange={checked =>
                    policy &&
                    updatePolicy({
                      ...policy,
                      prod_block_dangerous_sql: !!checked,
                    })
                  }
                />
                <span>
                  <span className="font-medium">{t('settings.safetyPolicyBlockDangerous')}</span>
                  <span className="block text-xs text-muted-foreground">
                    {t('settings.safetyPolicyBlockDangerousDescription')}
                  </span>
                </span>
              </label>

              <p className="text-xs text-muted-foreground">{t('settings.safetyPolicyNote')}</p>
              {policyError ? <p className="text-xs text-destructive">{policyError}</p> : null}
            </div>
          </SettingsCard>

          <ConfigBackupCard
            policy={policy}
            onApplyDiagnostics={updateDiagnostics}
            onApplyPolicy={updatePolicy}
            onApplyAnalyticsEnabled={(enabled: boolean) => {
              setAnalyticsEnabled(enabled);
              AnalyticsService.setAnalyticsEnabled(enabled);
            }}
          />

          <ProjectTransferCard projectId={DEFAULT_PROJECT_ID} />
        </div>
      </div>
    </div>
  );
}
