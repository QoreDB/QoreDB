import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useLicense } from '@/providers/LicenseProvider';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { LicenseBadge } from './LicenseBadge';

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
        {status.is_expired && (
          <span className="text-xs text-red-500">{t('license.expired')}</span>
        )}
      </div>

      {status.email && (
        <p className="text-xs text-[var(--color-text-tertiary)]">
          {t('license.licensedTo', { email: status.email })}
        </p>
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
          <Button
            onClick={handleActivate}
            disabled={loading || !key.trim()}
            size="sm"
          >
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

      {error && (
        <p className="text-xs text-red-500">{error}</p>
      )}
    </div>
  );
}
