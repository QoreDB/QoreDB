/**
 * Interceptor Settings Panel
 *
 * UI component for configuring the Universal Query Interceptor.
 * All data is stored and processed in the backend (Rust) for security.
 */

import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Shield,
  Activity,
  FileText,
  ChevronDown,
  ChevronRight,
  Plus,
  Trash2,
  RefreshCw,
} from 'lucide-react';
import { Switch } from '../ui/switch';
import { Label } from '../ui/label';
import { Input } from '../ui/input';
import { Button } from '../ui/button';
import {
  getInterceptorConfig,
  updateInterceptorConfig,
  getSafetyRules,
  addSafetyRule,
  removeSafetyRule,
  updateSafetyRule,
  type InterceptorConfig,
  type SafetyRule,
  BUILTIN_SAFETY_RULE_I18N,
} from '../../lib/tauri/interceptor';
import { SafetyRuleEditor } from './SafetyRuleEditor';
import { LicenseGate } from '@/components/License/LicenseGate';

interface SectionProps {
  title: string;
  description: string;
  icon: React.ReactNode;
  children: React.ReactNode;
  defaultOpen?: boolean;
}

function Section({ title, description, icon, children, defaultOpen = true }: SectionProps) {
  const [isOpen, setIsOpen] = useState(defaultOpen);

  return (
    <div className="border border-border rounded-lg overflow-hidden">
      <button
        type="button"
        className="w-full flex items-center gap-3 p-4 text-left hover:bg-muted/50 transition-colors"
        onClick={() => setIsOpen(!isOpen)}
      >
        <div className="p-2 rounded-lg bg-muted">{icon}</div>
        <div className="flex-1 min-w-0">
          <h3 className="font-medium text-sm">{title}</h3>
          <p className="text-xs text-muted-foreground truncate">{description}</p>
        </div>
        {isOpen ? (
          <ChevronDown className="w-4 h-4 text-muted-foreground" />
        ) : (
          <ChevronRight className="w-4 h-4 text-muted-foreground" />
        )}
      </button>
      {isOpen && <div className="p-4 pt-0 space-y-4">{children}</div>}
    </div>
  );
}

interface SettingRowProps {
  label: string;
  description?: string;
  children: React.ReactNode;
}

function SettingRow({ label, description, children }: SettingRowProps) {
  return (
    <div className="flex items-start justify-between gap-4 py-2">
      <div className="space-y-0.5 flex-1 min-w-0">
        <Label className="text-sm font-medium">{label}</Label>
        {description && <p className="text-xs text-muted-foreground">{description}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function InterceptorSettingsPanel() {
  const { t } = useTranslation();
  const [config, setConfig] = useState<InterceptorConfig | null>(null);
  const [rules, setRules] = useState<SafetyRule[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editingRule, setEditingRule] = useState<SafetyRule | null>(null);
  const [showRuleEditor, setShowRuleEditor] = useState(false);

  const getRuleLabels = useCallback(
    (rule: SafetyRule) => {
      if (rule.builtin) {
        const keys = BUILTIN_SAFETY_RULE_I18N[rule.id];
        if (keys) {
          return {
            name: t(keys.nameKey),
            description: t(keys.descriptionKey),
          };
        }
      }

      return { name: rule.name, description: rule.description };
    },
    [t]
  );

  // Load configuration from backend
  const loadConfig = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const [configData, rulesData] = await Promise.all([getInterceptorConfig(), getSafetyRules()]);
      setConfig(configData);
      setRules(rulesData);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load configuration');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  // Update config helper
  const updateConfig = useCallback(
    async (updates: Partial<InterceptorConfig>) => {
      if (!config) return;
      try {
        const newConfig = { ...config, ...updates };
        const updated = await updateInterceptorConfig(newConfig);
        setConfig(updated);
      } catch (err) {
        console.error('Failed to update config:', err);
      }
    },
    [config]
  );

  // Safety rule handlers
  const handleRuleSave = useCallback(
    async (rule: SafetyRule) => {
      try {
        if (editingRule) {
          const updated = await updateSafetyRule(rule);
          setRules(updated);
        } else {
          const updated = await addSafetyRule(rule);
          setRules(updated);
        }
        setShowRuleEditor(false);
        setEditingRule(null);
      } catch (err) {
        console.error('Failed to save rule:', err);
      }
    },
    [editingRule]
  );

  const handleRuleDelete = useCallback(async (ruleId: string) => {
    try {
      const updated = await removeSafetyRule(ruleId);
      setRules(updated);
    } catch (err) {
      console.error('Failed to delete rule:', err);
    }
  }, []);

  const handleRuleToggle = useCallback(async (rule: SafetyRule, enabled: boolean) => {
    try {
      const updated = await updateSafetyRule({ ...rule, enabled });
      setRules(updated);
    } catch (err) {
      console.error('Failed to toggle rule:', err);
    }
  }, []);

  const handleAddRule = useCallback(() => {
    setEditingRule(null);
    setShowRuleEditor(true);
  }, []);

  const handleEditRule = useCallback((rule: SafetyRule) => {
    setEditingRule(rule);
    setShowRuleEditor(true);
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center p-8">
        <RefreshCw className="w-5 h-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error || !config) {
    return (
      <div className="p-4 text-center">
        <p className="text-destructive mb-2">{error || 'Failed to load configuration'}</p>
        <Button variant="outline" size="sm" onClick={loadConfig}>
          {t('common.retry')}
        </Button>
      </div>
    );
  }

  // Separate built-in and custom rules
  const builtinRules = rules.filter(r => r.builtin);
  const customRules = rules.filter(r => !r.builtin);

  return (
    <div className="space-y-4">
      {/* Audit Logging */}
      <Section
        title={t('interceptor.audit.title')}
        description={t('interceptor.audit.description')}
        icon={<FileText className="w-4 h-4" />}
      >
        <SettingRow
          label={t('interceptor.audit.enabled')}
          description={t('interceptor.audit.enabledDescription')}
        >
          <Switch
            checked={config.audit_enabled}
            onCheckedChange={audit_enabled => updateConfig({ audit_enabled })}
          />
        </SettingRow>

        <SettingRow
          label={t('interceptor.audit.maxEntries')}
          description={t('interceptor.audit.maxEntriesDescription')}
        >
          <Input
            type="number"
            value={config.max_audit_entries}
            onChange={e => updateConfig({ max_audit_entries: parseInt(e.target.value) || 10000 })}
            className="w-24 h-8 text-sm"
            min={1000}
            max={100000}
            disabled={!config.audit_enabled}
          />
        </SettingRow>
      </Section>

      {/* Profiling (Pro) */}
      <LicenseGate feature="profiling">
      <Section
        title={t('interceptor.profiling.title')}
        description={t('interceptor.profiling.description')}
        icon={<Activity className="w-4 h-4" />}
      >
        <SettingRow
          label={t('interceptor.profiling.enabled')}
          description={t('interceptor.profiling.enabledDescription')}
        >
          <Switch
            checked={config.profiling_enabled}
            onCheckedChange={profiling_enabled => updateConfig({ profiling_enabled })}
          />
        </SettingRow>

        <SettingRow
          label={t('interceptor.profiling.slowQueryThreshold')}
          description={t('interceptor.profiling.slowQueryThresholdDescription')}
        >
          <div className="flex items-center gap-2">
            <Input
              type="number"
              value={config.slow_query_threshold_ms}
              onChange={e =>
                updateConfig({ slow_query_threshold_ms: parseInt(e.target.value) || 1000 })
              }
              className="w-24 h-8 text-sm"
              min={100}
              max={60000}
              step={100}
              disabled={!config.profiling_enabled}
            />
            <span className="text-sm text-muted-foreground">ms</span>
          </div>
        </SettingRow>

        <SettingRow
          label={t('interceptor.profiling.maxSlowQueries')}
          description={t('interceptor.profiling.maxSlowQueriesDescription')}
        >
          <Input
            type="number"
            value={config.max_slow_queries}
            onChange={e => updateConfig({ max_slow_queries: parseInt(e.target.value) || 100 })}
            className="w-24 h-8 text-sm"
            min={10}
            max={1000}
            disabled={!config.profiling_enabled}
          />
        </SettingRow>
      </Section>
      </LicenseGate>

      {/* Safety */}
      <Section
        title={t('interceptor.safety.title')}
        description={t('interceptor.safety.description')}
        icon={<Shield className="w-4 h-4" />}
      >
        <SettingRow
          label={t('interceptor.safety.enabled')}
          description={t('interceptor.safety.enabledDescription')}
        >
          <Switch
            checked={config.safety_enabled}
            onCheckedChange={safety_enabled => updateConfig({ safety_enabled })}
          />
        </SettingRow>

        {/* Built-in Rules */}
        <div className="pt-4 border-t border-border">
          <Label className="text-sm font-medium mb-3 block">
            {t('interceptor.safety.builtinRules')}
          </Label>
          <div className="space-y-2">
            {builtinRules.map(rule => (
              <div
                key={rule.id}
                className="flex items-center justify-between p-3 rounded-lg border border-border bg-muted/30"
              >
                <div className="flex items-center gap-3 flex-1 min-w-0">
                  <Switch
                    checked={rule.enabled}
                    onCheckedChange={enabled => handleRuleToggle(rule, enabled)}
                    disabled={!config.safety_enabled}
                  />
                  <div className="min-w-0">
                    <p className="text-sm font-medium truncate">{getRuleLabels(rule).name}</p>
                    <p className="text-xs text-muted-foreground truncate">
                      {getRuleLabels(rule).description}
                    </p>
                  </div>
                </div>
                <span className="text-xs bg-muted px-2 py-1 rounded">
                  {rule.action === 'block'
                    ? t('interceptor.safety.action.block')
                    : rule.action === 'warn'
                      ? t('interceptor.safety.action.warn')
                      : t('interceptor.safety.action.confirm')}
                </span>
              </div>
            ))}
          </div>
        </div>

        {/* Custom Rules (Pro) */}
        <LicenseGate feature="custom_safety_rules">
        <div className="pt-4 border-t border-border">
          <div className="flex items-center justify-between mb-3">
            <Label className="text-sm font-medium">{t('interceptor.safety.customRules')}</Label>
            <Button
              variant="outline"
              size="sm"
              onClick={handleAddRule}
              disabled={!config.safety_enabled}
            >
              <Plus className="w-3 h-3 mr-1" />
              {t('interceptor.safety.addRule')}
            </Button>
          </div>

          {customRules.length === 0 ? (
            <p className="text-sm text-muted-foreground text-center py-4">
              {t('interceptor.safety.noRules')}
            </p>
          ) : (
            <div className="space-y-2">
              {customRules.map(rule => (
                <div
                  key={rule.id}
                  className="flex items-center justify-between p-3 rounded-lg border border-border bg-muted/30"
                >
                  <div className="flex items-center gap-3 flex-1 min-w-0">
                    <Switch
                      checked={rule.enabled}
                      onCheckedChange={enabled => handleRuleToggle(rule, enabled)}
                      disabled={!config.safety_enabled}
                    />
                    <div className="min-w-0">
                      <p className="text-sm font-medium truncate">{getRuleLabels(rule).name}</p>
                      <p className="text-xs text-muted-foreground truncate">
                        {getRuleLabels(rule).description}
                      </p>
                    </div>
                  </div>
                  <div className="flex items-center gap-1 shrink-0">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleEditRule(rule)}
                      disabled={!config.safety_enabled}
                    >
                      {t('common.edit')}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleRuleDelete(rule.id)}
                      disabled={!config.safety_enabled}
                    >
                      <Trash2 className="w-4 h-4 text-destructive" />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
        </LicenseGate>
      </Section>

      {/* Rule Editor Modal */}
      {showRuleEditor && (
        <SafetyRuleEditor
          rule={editingRule}
          onSave={handleRuleSave}
          onCancel={() => {
            setShowRuleEditor(false);
            setEditingRule(null);
          }}
        />
      )}
    </div>
  );
}
