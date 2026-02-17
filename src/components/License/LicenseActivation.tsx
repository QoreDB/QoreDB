// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useLicense } from '@/providers/LicenseProvider';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { LicenseBadge } from './LicenseBadge';

function formatDate(iso: string | null): string {
  if (!iso) return '—';
  try {
    return new Date(iso).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'long',
      day: 'numeric',
    });
  } catch {
    return iso;
  }
}

/**
 * License activation/deactivation UI for Settings page.
 */
export function LicenseActivation() {
  const { t } = useTranslation();
  const { status, activate, deactivate } = useLicense();
  const [key, setKey] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const handleActivate = async () => {
    setError(null);
    setLoading(true);
    try {
      await activate(key.trim());
      setKey('');
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleDeactivate = async () => {
    setError(null);
    setLoading(true);
    try {
      await deactivate();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const isActive = status.tier !== 'core' && !status.is_expired;
  const hasLicenseInfo = status.email || status.payment_id;

  return (
    <div className="flex flex-col gap-4">
      {/* Current status */}
      <div className="flex items-center gap-2">
        <span className="text-sm font-medium text-[var(--color-text-primary)]">
          {t('license.currentTier')}:
        </span>
        {isActive ? (
          <LicenseBadge tier={status.tier} />
        ) : (
          <span className="text-sm text-[var(--color-text-secondary)]">Core</span>
        )}
        {status.is_expired && <span className="text-xs text-red-500">{t('license.expired')}</span>}
      </div>

      {/* License details */}
      {hasLicenseInfo && (
        <div className="flex flex-col gap-1 rounded-md border border-[var(--color-border)] p-3 text-xs">
          {status.email && (
            <div className="flex gap-2">
              <span className="text-[var(--color-text-tertiary)]">{t('license.email')}:</span>
              <span className="text-[var(--color-text-secondary)]">{status.email}</span>
            </div>
          )}
          {status.payment_id && (
            <div className="flex gap-2">
              <span className="text-[var(--color-text-tertiary)]">{t('license.paymentId')}:</span>
              <span className="font-mono text-[var(--color-text-secondary)]">
                {status.payment_id}
              </span>
            </div>
          )}
          {status.issued_at && (
            <div className="flex gap-2">
              <span className="text-[var(--color-text-tertiary)]">{t('license.issuedAt')}:</span>
              <span className="text-[var(--color-text-secondary)]">
                {formatDate(status.issued_at)}
              </span>
            </div>
          )}
          <div className="flex gap-2">
            <span className="text-[var(--color-text-tertiary)]">{t('license.expiresAt')}:</span>
            <span className="text-[var(--color-text-secondary)]">
              {status.expires_at ? formatDate(status.expires_at) : t('license.perpetual')}
            </span>
          </div>
        </div>
      )}

      {/* Activation form */}
      {!isActive && (
        <div className="flex gap-2">
          <Input
            value={key}
            onChange={e => setKey(e.target.value)}
            placeholder={t('license.keyPlaceholder')}
            className="flex-1 font-mono text-xs"
          />
          <Button onClick={handleActivate} disabled={loading || !key.trim()} size="sm">
            {t('license.activate')}
          </Button>
        </div>
      )}

      {/* Deactivation */}
      {isActive && (
        <Button
          variant="ghost"
          size="sm"
          onClick={handleDeactivate}
          disabled={loading}
          className="w-fit text-xs"
        >
          {t('license.deactivate')}
        </Button>
      )}

      {error && <p className="text-xs text-red-500">{error}</p>}
    </div>
  );
}
