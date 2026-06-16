// SPDX-License-Identifier: Apache-2.0

import { KeyRound, RefreshCw, ShieldCheck, UserPlus } from 'lucide-react';
import { type FormEvent, useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { notify } from '@/lib/notify';
import { type AdminUser, webCreateUser, webListUsers, webResetPassword } from '@/lib/transport';
import { SettingsCard } from '../SettingsCard';

const MIN_PASSWORD = 8;

interface AdminSectionProps {
  searchQuery?: string;
}

export function AdminSection({ searchQuery }: AdminSectionProps) {
  const { t } = useTranslation();
  const [users, setUsers] = useState<AdminUser[]>([]);
  const [loading, setLoading] = useState(true);

  const [newEmail, setNewEmail] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [newIsAdmin, setNewIsAdmin] = useState(false);
  const [creating, setCreating] = useState(false);

  const [resetEmail, setResetEmail] = useState<string | null>(null);
  const [resetPassword, setResetPassword] = useState('');
  const [resetting, setResetting] = useState(false);

  const loadUsers = useCallback(async () => {
    setLoading(true);
    try {
      setUsers(await webListUsers());
    } catch (err) {
      notify.error(t('settings.admin.loadFailed'), err);
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    loadUsers();
  }, [loadUsers]);

  const createUser = async (e: FormEvent) => {
    e.preventDefault();
    if (newPassword.length < MIN_PASSWORD) {
      notify.error(t('auth.passwordTooShort'));
      return;
    }
    setCreating(true);
    try {
      await webCreateUser(newEmail, newPassword, newIsAdmin);
      notify.success(t('settings.admin.userCreated'));
      setNewEmail('');
      setNewPassword('');
      setNewIsAdmin(false);
      await loadUsers();
    } catch (err) {
      notify.error(t('settings.admin.createFailed'), err);
    } finally {
      setCreating(false);
    }
  };

  const submitReset = async (e: FormEvent) => {
    e.preventDefault();
    if (!resetEmail) return;
    if (resetPassword.length < MIN_PASSWORD) {
      notify.error(t('auth.passwordTooShort'));
      return;
    }
    setResetting(true);
    try {
      await webResetPassword(resetEmail, resetPassword);
      notify.success(t('settings.admin.passwordReset', { email: resetEmail }));
      setResetEmail(null);
      setResetPassword('');
    } catch (err) {
      notify.error(t('settings.admin.resetFailed'), err);
    } finally {
      setResetting(false);
    }
  };

  return (
    <div className="space-y-6">
      <SettingsCard
        title={t('settings.admin.usersTitle')}
        description={t('settings.admin.usersDescription')}
        searchQuery={searchQuery}
      >
        {loading ? (
          <p className="text-sm text-muted-foreground">{t('settings.admin.loading')}</p>
        ) : users.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t('settings.admin.noUsers')}</p>
        ) : (
          <ul className="divide-y divide-border/50 rounded-md border border-border/50">
            {users.map(user => (
              <li key={user.id} className="flex flex-col gap-2 p-3">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex min-w-0 items-center gap-2">
                    <span className="truncate text-sm">{user.email}</span>
                    {user.is_admin && (
                      <span className="inline-flex items-center gap-1 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider text-primary">
                        <ShieldCheck size={11} />
                        {t('settings.admin.adminBadge')}
                      </span>
                    )}
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => {
                      setResetEmail(prev => (prev === user.email ? null : user.email));
                      setResetPassword('');
                    }}
                  >
                    <KeyRound size={13} />
                    {t('settings.admin.resetPassword')}
                  </Button>
                </div>
                {resetEmail === user.email && (
                  <form onSubmit={submitReset} className="flex items-end gap-2">
                    <div className="flex flex-1 flex-col gap-1">
                      <Label htmlFor={`reset-${user.id}`} className="text-xs">
                        {t('settings.admin.newPassword')}
                      </Label>
                      <Input
                        id={`reset-${user.id}`}
                        type="password"
                        value={resetPassword}
                        onChange={e => setResetPassword(e.target.value)}
                        autoComplete="new-password"
                        // biome-ignore lint/a11y/noAutofocus: focus the freshly revealed field
                        autoFocus
                      />
                    </div>
                    <Button type="submit" size="sm" disabled={resetting}>
                      {resetting ? t('auth.submitting') : t('settings.admin.apply')}
                    </Button>
                  </form>
                )}
              </li>
            ))}
          </ul>
        )}
        <div className="mt-3">
          <Button type="button" variant="ghost" size="sm" onClick={loadUsers} disabled={loading}>
            <RefreshCw size={13} />
            {t('settings.admin.refresh')}
          </Button>
        </div>
      </SettingsCard>

      <SettingsCard
        title={t('settings.admin.createTitle')}
        description={t('settings.admin.createDescription')}
        searchQuery={searchQuery}
      >
        <form onSubmit={createUser} className="flex flex-col gap-3">
          <div className="flex flex-col gap-1">
            <Label htmlFor="admin-new-email">{t('auth.email')}</Label>
            <Input
              id="admin-new-email"
              type="email"
              value={newEmail}
              onChange={e => setNewEmail(e.target.value)}
              autoComplete="off"
              required
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="admin-new-password">{t('auth.password')}</Label>
            <Input
              id="admin-new-password"
              type="password"
              value={newPassword}
              onChange={e => setNewPassword(e.target.value)}
              autoComplete="new-password"
              required
            />
          </div>
          <div className="flex items-center gap-2">
            <Switch checked={newIsAdmin} onCheckedChange={setNewIsAdmin} />
            <span className="text-sm">{t('settings.admin.grantAdmin')}</span>
          </div>
          <Button type="submit" disabled={creating} className="self-start">
            <UserPlus size={14} />
            {creating ? t('auth.submitting') : t('settings.admin.createUser')}
          </Button>
        </form>
      </SettingsCard>
    </div>
  );
}
