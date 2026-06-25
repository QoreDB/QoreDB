// SPDX-License-Identifier: Apache-2.0

import { CreditCard, ExternalLink, RefreshCw } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DangerConfirmDialog } from '@/components/Guard/DangerConfirmDialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { licenseErrorKey } from '@/lib/license';
import { openExternal } from '@/lib/transport';
import { useLicense } from '@/providers/LicenseProvider';
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

export function LicenseActivation() {
  const { t } = useTranslation();
  const { status, activate, deactivate, refresh, openBillingPortal } = useLicense();
  const [key, setKey] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState<'refresh' | 'billing' | null>(null);
  const [showDeactivateConfirm, setShowDeactivateConfirm] = useState(false);

  const handleActivate = async () => {
    setError(null);
    setLoading(true);
    try {
      await activate(key.trim());
      setKey('');
    } catch (e) {
      setError(t(licenseErrorKey(e)));
    } finally {
      setLoading(false);
    }
  };

  const handleDeactivate = async () => {
    setError(null);
    setLoading(true);
    try {
      await deactivate();
      setShowDeactivateConfirm(false);
    } catch (e) {
      setError(t(licenseErrorKey(e)));
    } finally {
      setLoading(false);
    }
  };

  const handleRefresh = async () => {
    setError(null);
    setBusy('refresh');
    try {
      await refresh();
    } catch (e) {
      setError(t(licenseErrorKey(e)));
    } finally {
      setBusy(null);
    }
  };

  const handleBilling = async () => {
    setError(null);
    setBusy('billing');
    try {
      await openBillingPortal();
    } catch (e) {
      setError(t(licenseErrorKey(e)));
    } finally {
      setBusy(null);
    }
  };

  const isActive = status.tier !== 'core' && !status.is_expired;
  // `seats` is retained even on an expired license, so it doubles as a Team marker.
  const isTeam = status.seats != null;
  const hasLicenseInfo = status.email || status.payment_id;
  const taglineKey = `license.tierTagline.${isActive ? status.tier : 'core'}`;
  const tagline = t(taglineKey, { defaultValue: '' });

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-[var(--color-text-primary)]">
            {t('license.currentTier')}:
          </span>
          {isActive ? (
            <LicenseBadge tier={status.tier} />
          ) : (
            <span className="text-sm text-[var(--color-text-secondary)]">Free</span>
          )}
          {status.is_expired && (
            <span className="text-xs text-red-500">{t('license.expired')}</span>
          )}
        </div>
        {tagline && <p className="text-xs text-[var(--color-text-secondary)]">{tagline}</p>}
      </div>

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
          {status.seats != null && (
            <div className="flex gap-2">
              <span className="text-[var(--color-text-tertiary)]">{t('license.seats')}:</span>
              <span className="text-[var(--color-text-secondary)]">
                {t('license.seatsCount', { count: status.seats })}
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

      {/* Expired license: invite to refresh instead of a hard error */}
      {status.is_expired && (
        <div className="flex flex-col gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-xs">
          <span className="text-amber-700 dark:text-amber-400">{t('license.expiredBanner')}</span>
          {isTeam && status.email && (
            <Button
              variant="outline"
              size="sm"
              onClick={handleRefresh}
              disabled={busy !== null}
              className="w-fit gap-1.5 text-xs"
            >
              <RefreshCw size={12} className={busy === 'refresh' ? 'animate-spin' : ''} />
              {t('license.refresh')}
            </Button>
          )}
        </div>
      )}

      {!isActive && (
        <>
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
          <Button
            variant="link"
            size="sm"
            className="w-fit gap-1.5 px-0 text-xs text-accent"
            onClick={() => openExternal('https://qoredb.com/pricing')}
          >
            <ExternalLink size={12} />
            {t('license.getPro')}
          </Button>
        </>
      )}

      {isActive && isTeam && (
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleRefresh}
            disabled={busy !== null}
            className="gap-1.5 text-xs"
          >
            <RefreshCw size={12} className={busy === 'refresh' ? 'animate-spin' : ''} />
            {t('license.refresh')}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={handleBilling}
            disabled={busy !== null}
            className="gap-1.5 text-xs"
          >
            <CreditCard size={12} />
            {t('license.manageBilling')}
          </Button>
        </div>
      )}

      {isActive && (
        <>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setShowDeactivateConfirm(true)}
            disabled={loading}
            className="w-fit text-xs"
          >
            {t('license.deactivate')}
          </Button>
          <DangerConfirmDialog
            open={showDeactivateConfirm}
            onOpenChange={setShowDeactivateConfirm}
            title={t('license.deactivateConfirm.title')}
            description={t('license.deactivateConfirm.description')}
            confirmLabel={t('license.deactivateConfirm.confirm')}
            loading={loading}
            onConfirm={handleDeactivate}
          />
        </>
      )}

      {error && <p className="text-xs text-red-500">{error}</p>}
    </div>
  );
}
