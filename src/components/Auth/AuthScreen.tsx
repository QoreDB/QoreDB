// SPDX-License-Identifier: Apache-2.0

import { type FormEvent, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { LanguageSwitcher } from '@/components/ui/language-switcher';
import { type AuthStatus, webLogin, webRegister, webSsoStart } from '@/lib/transport';

const MIN_PASSWORD = 8;

interface AuthScreenProps {
  status: AuthStatus;
  ssoError?: string;
  onAuthenticated: () => void;
}

export function AuthScreen({ status, ssoError, onAuthenticated }: AuthScreenProps) {
  const { t } = useTranslation();
  const setup = status.setupRequired;

  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [busy, setBusy] = useState(false);
  const [showForgot, setShowForgot] = useState(false);
  const [error, setError] = useState<string | undefined>(
    ssoError ? t('auth.ssoFailed', { error: ssoError }) : undefined
  );

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);

    if (setup) {
      if (password.length < MIN_PASSWORD) {
        setError(t('auth.passwordTooShort'));
        return;
      }
      if (password !== confirm) {
        setError(t('auth.passwordMismatch'));
        return;
      }
    }

    setBusy(true);
    try {
      if (setup) await webRegister(email, password);
      await webLogin(email, password);
      onAuthenticated();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/95 p-4 backdrop-blur-sm">
      <div className="absolute right-4 top-4">
        <LanguageSwitcher />
      </div>

      <div className="w-full max-w-sm rounded-xl border bg-card p-8 shadow-2xl">
        <div className="mb-6 flex flex-col items-center gap-3 text-center">
          <img src="/logo.png" alt="QoreDB" width={48} height={48} />
          <div className="flex flex-col gap-1">
            <h1 className="text-xl font-semibold tracking-tight">
              {t(setup ? 'auth.setupTitle' : 'auth.loginTitle')}
            </h1>
            <p className="text-sm text-muted-foreground">
              {t(setup ? 'auth.setupSubtitle' : 'auth.loginSubtitle')}
            </p>
          </div>
        </div>

        <form onSubmit={submit} className="flex flex-col gap-4">
          <div className="flex flex-col">
            <Label htmlFor="auth-email">{t('auth.email')}</Label>
            <Input
              id="auth-email"
              type="email"
              value={email}
              onChange={e => setEmail(e.target.value)}
              autoComplete="email"
              required
              autoFocus
            />
          </div>

          <div className="flex flex-col">
            <Label htmlFor="auth-password">{t('auth.password')}</Label>
            <Input
              id="auth-password"
              type="password"
              value={password}
              onChange={e => setPassword(e.target.value)}
              autoComplete={setup ? 'new-password' : 'current-password'}
              required
            />
          </div>

          {setup && (
            <div className="flex flex-col">
              <Label htmlFor="auth-confirm">{t('auth.confirmPassword')}</Label>
              <Input
                id="auth-confirm"
                type="password"
                value={confirm}
                onChange={e => setConfirm(e.target.value)}
                autoComplete="new-password"
                required
              />
            </div>
          )}

          {!setup && (
            <div className="-mt-2 flex flex-col items-end gap-1">
              <button
                type="button"
                className="text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
                onClick={() => setShowForgot(v => !v)}
              >
                {t('auth.forgotPassword')}
              </button>
              {showForgot && (
                <p className="text-xs text-muted-foreground text-right">
                  {t('auth.forgotPasswordHelp')}
                </p>
              )}
            </div>
          )}

          {error && (
            <p className="text-sm text-[var(--color-error)]" role="alert">
              {error}
            </p>
          )}

          <Button type="submit" disabled={busy} className="mt-2 w-full">
            {busy ? t('auth.submitting') : t(setup ? 'auth.createAdmin' : 'auth.signIn')}
          </Button>
        </form>

        {!setup && status.ssoEnabled && (
          <>
            <div className="my-5 flex items-center gap-3">
              <span className="h-px flex-1 bg-border" />
              <span className="text-xs uppercase tracking-wider text-muted-foreground">
                {t('auth.or')}
              </span>
              <span className="h-px flex-1 bg-border" />
            </div>
            <Button
              type="button"
              variant="outline"
              className="w-full"
              onClick={() => webSsoStart()}
            >
              {t('auth.ssoButton')}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
