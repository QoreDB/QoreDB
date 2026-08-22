// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { InterceptorSettingsPanel } from '@/components/Interceptor';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { SecretPolicy } from '@/lib/replay';
import { getSecretPolicy, setSecretPolicy } from '@/lib/replayPreferences';
import { getSafetyPolicy, type SafetyPolicy, setSafetyPolicy } from '@/lib/tauri';
import { useLicense } from '@/providers/LicenseProvider';
import { SettingsCard } from '../SettingsCard';

interface SecuritySectionProps {
  searchQuery?: string;
}

// Default values for detecting modifications
const DEFAULTS = {
  prod_require_confirmation: true,
  prod_block_dangerous_sql: false,
  query_rate_limit_enabled: true,
};

export function SecuritySection({ searchQuery }: SecuritySectionProps) {
  const { t } = useTranslation();
  const [policy, setPolicy] = useState<SafetyPolicy | null>(null);
  const [secretPolicy, setSecretPolicyState] = useState<SecretPolicy>(getSecretPolicy);
  const { isFeatureEnabled } = useLicense();
  const [policyError, setPolicyError] = useState<string | null>(null);
  const [policySaving, setPolicySaving] = useState(false);

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

  const isPolicyModified =
    policy &&
    (policy.prod_require_confirmation !== DEFAULTS.prod_require_confirmation ||
      policy.prod_block_dangerous_sql !== DEFAULTS.prod_block_dangerous_sql ||
      (policy.query_rate_limit_enabled ?? true) !== DEFAULTS.query_rate_limit_enabled);

  return (
    <>
      <SettingsCard
        id="safety-policy"
        title={t('settings.safetyPolicy')}
        description={t('settings.safetyPolicyDescription')}
        isModified={!!isPolicyModified}
        searchQuery={searchQuery}
      >
        <div className="space-y-3">
          <label className="flex items-start gap-2.5 text-sm cursor-pointer">
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
              className="mt-0.5"
            />
            <span>
              <span className="font-medium text-foreground">
                {t('settings.safetyPolicyRequireConfirmation')}
              </span>
              <span className="block text-xs text-muted-foreground mt-0.5">
                {t('settings.safetyPolicyRequireConfirmationDescription')}
              </span>
            </span>
          </label>

          <label className="flex items-start gap-2.5 text-sm cursor-pointer">
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
              className="mt-0.5"
            />
            <span>
              <span className="font-medium text-foreground">
                {t('settings.safetyPolicyBlockDangerous')}
              </span>
              <span className="block text-xs text-muted-foreground mt-0.5">
                {t('settings.safetyPolicyBlockDangerousDescription')}
              </span>
            </span>
          </label>

          <label className="flex items-start gap-2.5 text-sm cursor-pointer">
            <Checkbox
              checked={policy?.query_rate_limit_enabled ?? true}
              disabled={!policy || policySaving}
              onCheckedChange={checked =>
                policy &&
                updatePolicy({
                  ...policy,
                  query_rate_limit_enabled: !!checked,
                })
              }
              className="mt-0.5"
            />
            <span>
              <span className="font-medium text-foreground">
                {t('settings.safetyPolicyRateLimit')}
              </span>
              <span className="block text-xs text-muted-foreground mt-0.5">
                {t('settings.safetyPolicyRateLimitDescription')}
              </span>
            </span>
          </label>

          <p className="text-xs text-muted-foreground pt-1">{t('settings.safetyPolicyNote')}</p>
          {policyError ? <p className="text-xs text-destructive">{policyError}</p> : null}
        </div>
      </SettingsCard>

      {isFeatureEnabled('query_replay') && (
        <SettingsCard
          id="replay-secrets"
          title={t('replay.settings.title')}
          description={t('replay.settings.description')}
          searchQuery={searchQuery}
        >
          <div className="space-y-2">
            <Label htmlFor="replay-secret-policy" className="text-xs">
              {t('replay.settings.policyLabel')}
            </Label>
            <Select
              value={secretPolicy}
              onValueChange={value => {
                const next = value as SecretPolicy;
                setSecretPolicyState(next);
                setSecretPolicy(next);
              }}
            >
              <SelectTrigger id="replay-secret-policy" className="h-8 w-72 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="off">{t('replay.settings.off')}</SelectItem>
                <SelectItem value="warn">{t('replay.settings.warn')}</SelectItem>
                <SelectItem value="redact">{t('replay.settings.redact')}</SelectItem>
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              {t(`replay.settings.${secretPolicy}Hint`)}
            </p>
          </div>
        </SettingsCard>
      )}

      <SettingsCard
        id="interceptor"
        title={t('interceptor.title')}
        description={t('interceptor.description')}
        searchQuery={searchQuery}
      >
        <InterceptorSettingsPanel />
      </SettingsCard>
    </>
  );
}
