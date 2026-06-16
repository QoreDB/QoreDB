// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, ChevronRight, Info } from 'lucide-react';
import { useId, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { cn } from '@/lib/utils';

import { Field } from './Field';
import type { ConnectionFormData } from './types';

const SMALL_LABEL = 'text-xs text-muted-foreground';

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
  const hostKeyPolicyId = useId();

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
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-auto px-0 text-sm font-medium text-muted-foreground hover:text-foreground"
              onClick={() => setIsOpen(v => !v)}
            >
              {isOpen ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
              {t('connection.ssh.configure')}
            </Button>
          </div>

          {isOpen && (
            <div className="px-4 pb-4 space-y-4">
              <div className="grid grid-cols-3 gap-3">
                <Field
                  label={t('connection.ssh.host')}
                  className="col-span-2"
                  labelClassName={SMALL_LABEL}
                >
                  <Input
                    placeholder="bastion.example.com"
                    value={formData.sshHost}
                    onChange={e => onChange('sshHost', e.target.value)}
                  />
                </Field>
                <Field label={t('connection.ssh.port')} labelClassName={SMALL_LABEL}>
                  <Input
                    type="number"
                    value={formData.sshPort}
                    onChange={e => onChange('sshPort', parseIntOr(e.target.value, 22))}
                  />
                </Field>
              </div>

              <Field label={t('connection.ssh.username')} labelClassName={SMALL_LABEL}>
                <Input
                  placeholder="ssh_user"
                  value={formData.sshUsername}
                  onChange={e => onChange('sshUsername', e.target.value)}
                />
              </Field>

              <Field label={t('connection.ssh.keyPath')} labelClassName={SMALL_LABEL}>
                <Input
                  placeholder={t('connection.ssh.keyPathPlaceholder')}
                  value={formData.sshKeyPath}
                  onChange={e => onChange('sshKeyPath', e.target.value)}
                />
              </Field>

              <div className="flex items-start gap-2 rounded-md border border-border bg-muted/30 p-2">
                <Info size={14} className="mt-0.5 shrink-0 text-muted-foreground" />
                <div className="space-y-1">
                  <p className="text-xs font-medium text-foreground">
                    {t('connection.ssh.sshAgentInfo')}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {t('connection.ssh.sshAgentHint')}
                  </p>
                </div>
              </div>

              <div className="rounded-md border border-border">
                <Button
                  type="button"
                  variant="ghost"
                  className={cn(
                    'h-auto flex w-full items-center justify-between px-3 py-2 text-sm font-medium',
                    'hover:bg-muted/40 transition-colors'
                  )}
                  onClick={() => setShowAdvanced(v => !v)}
                >
                  <span className="flex items-center gap-2">
                    {showAdvanced ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
                    {t('connection.ssh.advancedOptions')}
                  </span>
                </Button>

                {showAdvanced && (
                  <div className="px-3 pb-3 space-y-3 border-t border-border">
                    <div className="grid grid-cols-2 gap-3 pt-3">
                      <div className="space-y-2">
                        <Label htmlFor={hostKeyPolicyId} className="text-xs text-muted-foreground">
                          {t('connection.ssh.hostKeyPolicy')}
                        </Label>
                        <Select
                          value={formData.sshHostKeyPolicy}
                          onValueChange={value =>
                            onChange(
                              'sshHostKeyPolicy',
                              value as ConnectionFormData['sshHostKeyPolicy']
                            )
                          }
                        >
                          <SelectTrigger id={hostKeyPolicyId} className="h-9 w-full">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="accept_new">
                              {t('connection.ssh.hostKeyPolicyAcceptNew')}
                            </SelectItem>
                            <SelectItem value="strict">
                              {t('connection.ssh.hostKeyPolicyStrict')}
                            </SelectItem>
                            <SelectItem value="insecure_no_check">
                              {t('connection.ssh.hostKeyPolicyInsecure')}
                            </SelectItem>
                          </SelectContent>
                        </Select>
                      </div>
                      <Field label={t('connection.ssh.proxyJump')} labelClassName={SMALL_LABEL}>
                        <Input
                          placeholder="user@bastion:22"
                          value={formData.sshProxyJump}
                          onChange={e => onChange('sshProxyJump', e.target.value)}
                        />
                      </Field>
                    </div>

                    <div className="grid grid-cols-3 gap-3">
                      <Field
                        label={t('connection.ssh.connectTimeoutSecs')}
                        labelClassName={SMALL_LABEL}
                      >
                        <Input
                          type="number"
                          min={1}
                          value={formData.sshConnectTimeoutSecs}
                          onChange={e =>
                            onChange('sshConnectTimeoutSecs', parseIntOr(e.target.value, 10))
                          }
                        />
                      </Field>
                      <Field
                        label={t('connection.ssh.keepaliveIntervalSecs')}
                        labelClassName={SMALL_LABEL}
                      >
                        <Input
                          type="number"
                          min={0}
                          value={formData.sshKeepaliveIntervalSecs}
                          onChange={e =>
                            onChange('sshKeepaliveIntervalSecs', parseIntOr(e.target.value, 30))
                          }
                        />
                      </Field>
                      <Field
                        label={t('connection.ssh.keepaliveCountMax')}
                        labelClassName={SMALL_LABEL}
                      >
                        <Input
                          type="number"
                          min={0}
                          value={formData.sshKeepaliveCountMax}
                          onChange={e =>
                            onChange('sshKeepaliveCountMax', parseIntOr(e.target.value, 3))
                          }
                        />
                      </Field>
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
