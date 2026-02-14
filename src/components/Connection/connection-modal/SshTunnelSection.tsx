import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronDown, ChevronRight, Info } from 'lucide-react';

import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { cn } from '@/lib/utils';

import type { ConnectionFormData } from './types';

function parseIntOr(value: string, fallback: number) {
  const n = parseInt(value, 10);
  return Number.isFinite(n) ? n : fallback;
}

function getPathBasename(path: string): string {
  const normalized = path.replace(/\\/g, '/');
  const parts = normalized.split('/').filter(Boolean);
  return parts.length ? parts[parts.length - 1] : path;
}

export function SshTunnelSection(props: {
  formData: ConnectionFormData;
  onChange: (field: keyof ConnectionFormData, value: string | number | boolean) => void;
}) {
  const { formData, onChange } = props;
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);

  const summary = useMemo(() => {
    if (!formData.useSshTunnel) return '';
    const policyLabel =
      formData.sshHostKeyPolicy === 'accept_new'
        ? t('connection.ssh.hostKeyPolicyAcceptNew')
        : formData.sshHostKeyPolicy === 'strict'
          ? t('connection.ssh.hostKeyPolicyStrict')
          : t('connection.ssh.hostKeyPolicyInsecure');

    const hostPart = formData.sshHost
      ? `${formData.sshHost}:${formData.sshPort || 22}`
      : t('connection.ssh.summaryMissingHost');
    const userPrefix = formData.sshUsername ? `${formData.sshUsername}@` : '';
    const keyPart = formData.sshKeyPath
      ? `${t('connection.ssh.summaryKey')} ${getPathBasename(formData.sshKeyPath)}`
      : t('connection.ssh.summaryMissingKey');

    return `${userPrefix}${hostPart} · ${keyPart} · ${policyLabel}`;
  }, [formData, t]);

  return (
    <div className="rounded-md border border-border bg-background">
      <div className="flex items-center justify-between px-3 py-2">
        <div className="space-y-1">
          <Label className="text-sm">{t('connection.ssh.enableTunnel')}</Label>
          {formData.useSshTunnel && <p className="text-xs text-muted-foreground">{summary}</p>}
        </div>
        <Switch
          checked={formData.useSshTunnel}
          onCheckedChange={checked => {
            onChange('useSshTunnel', checked);
            if (checked) setIsOpen(true);
          }}
        />
      </div>

      {formData.useSshTunnel && (
        <div className="border-t border-border">
          <div className="flex items-center justify-between px-4 py-2">
            <button
              type="button"
              className="flex items-center gap-2 text-sm font-medium text-muted-foreground hover:text-foreground"
              onClick={() => setIsOpen(v => !v)}
            >
              {isOpen ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
              {t('connection.ssh.configure')}
            </button>
          </div>

          {isOpen && (
            <div className="px-4 pb-4 space-y-4">
              <div className="grid grid-cols-3 gap-3">
                <div className="col-span-2 space-y-2">
                  <Label className="text-xs text-muted-foreground">
                    {t('connection.ssh.host')}
                  </Label>
                  <Input
                    placeholder="bastion.example.com"
                    value={formData.sshHost}
                    onChange={e => onChange('sshHost', e.target.value)}
                  />
                </div>
                <div className="space-y-2">
                  <Label className="text-xs text-muted-foreground">
                    {t('connection.ssh.port')}
                  </Label>
                  <Input
                    type="number"
                    value={formData.sshPort}
                    onChange={e => onChange('sshPort', parseIntOr(e.target.value, 22))}
                  />
                </div>
              </div>

              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">
                  {t('connection.ssh.username')}
                </Label>
                <Input
                  placeholder="ssh_user"
                  value={formData.sshUsername}
                  onChange={e => onChange('sshUsername', e.target.value)}
                />
              </div>

              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">
                  {t('connection.ssh.keyPath')}
                </Label>
                <Input
                  placeholder={t('connection.ssh.keyPathPlaceholder')}
                  value={formData.sshKeyPath}
                  onChange={e => onChange('sshKeyPath', e.target.value)}
                />
              </div>

              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">
                  {t('connection.ssh.passphrase')}
                </Label>
                <Input
                  type="password"
                  placeholder="••••••••"
                  value={formData.sshPassphrase}
                  onChange={e => onChange('sshPassphrase', e.target.value)}
                />
                <div className="flex items-start gap-2 rounded-md border border-border bg-muted/30 p-2">
                  <Info size={14} className="mt-0.5 text-muted-foreground" />
                  <p className="text-xs text-muted-foreground">
                    {t('connection.ssh.passphraseHelp')}
                  </p>
                </div>
              </div>

              <div className="rounded-md border border-border">
                <button
                  type="button"
                  className={cn(
                    'flex w-full items-center justify-between px-3 py-2 text-sm font-medium',
                    'hover:bg-muted/40 transition-colors'
                  )}
                  onClick={() => setShowAdvanced(v => !v)}
                >
                  <span className="flex items-center gap-2">
                    {showAdvanced ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
                    {t('connection.ssh.advancedOptions')}
                  </span>
                </button>

                {showAdvanced && (
                  <div className="px-3 pb-3 space-y-3 border-t border-border">
                    <div className="grid grid-cols-2 gap-3 pt-3">
                      <div className="space-y-2">
                        <Label className="text-xs text-muted-foreground">
                          {t('connection.ssh.hostKeyPolicy')}
                        </Label>
                        <select
                          className="w-full h-9 rounded-md border border-border bg-background px-3 text-sm"
                          value={formData.sshHostKeyPolicy}
                          onChange={e =>
                            onChange(
                              'sshHostKeyPolicy',
                              e.target.value as ConnectionFormData['sshHostKeyPolicy']
                            )
                          }
                        >
                          <option value="accept_new">
                            {t('connection.ssh.hostKeyPolicyAcceptNew')}
                          </option>
                          <option value="strict">{t('connection.ssh.hostKeyPolicyStrict')}</option>
                          <option value="insecure_no_check">
                            {t('connection.ssh.hostKeyPolicyInsecure')}
                          </option>
                        </select>
                      </div>
                      <div className="space-y-2">
                        <Label className="text-xs text-muted-foreground">
                          {t('connection.ssh.proxyJump')}
                        </Label>
                        <Input
                          placeholder="user@bastion:22"
                          value={formData.sshProxyJump}
                          onChange={e => onChange('sshProxyJump', e.target.value)}
                        />
                      </div>
                    </div>

                    <div className="grid grid-cols-3 gap-3">
                      <div className="space-y-2">
                        <Label className="text-xs text-muted-foreground">
                          {t('connection.ssh.connectTimeoutSecs')}
                        </Label>
                        <Input
                          type="number"
                          min={1}
                          value={formData.sshConnectTimeoutSecs}
                          onChange={e =>
                            onChange('sshConnectTimeoutSecs', parseIntOr(e.target.value, 10))
                          }
                        />
                      </div>
                      <div className="space-y-2">
                        <Label className="text-xs text-muted-foreground">
                          {t('connection.ssh.keepaliveIntervalSecs')}
                        </Label>
                        <Input
                          type="number"
                          min={0}
                          value={formData.sshKeepaliveIntervalSecs}
                          onChange={e =>
                            onChange('sshKeepaliveIntervalSecs', parseIntOr(e.target.value, 30))
                          }
                        />
                      </div>
                      <div className="space-y-2">
                        <Label className="text-xs text-muted-foreground">
                          {t('connection.ssh.keepaliveCountMax')}
                        </Label>
                        <Input
                          type="number"
                          min={0}
                          value={formData.sshKeepaliveCountMax}
                          onChange={e =>
                            onChange('sshKeepaliveCountMax', parseIntOr(e.target.value, 3))
                          }
                        />
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
