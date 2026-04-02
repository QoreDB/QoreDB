// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, ChevronRight } from 'lucide-react';
import { useMemo, useState } from 'react';
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

import type { ConnectionFormData } from './types';

function parseIntOr(value: string, fallback: number) {
  const n = parseInt(value, 10);
  return Number.isFinite(n) ? n : fallback;
}

export function ProxySection(props: {
  formData: ConnectionFormData;
  onChange: (field: keyof ConnectionFormData, value: string | number | boolean) => void;
}) {
  const { formData, onChange } = props;
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const [showAuth, setShowAuth] = useState(false);

  const summary = useMemo(() => {
    if (!formData.useProxy) return '';
    const typePart = formData.proxyType === 'socks5' ? 'SOCKS5' : 'HTTP CONNECT';
    const hostPart = formData.proxyHost
      ? `${formData.proxyHost}:${formData.proxyPort}`
      : t('connection.proxy.summaryMissingHost');
    const authPart = formData.proxyUsername ? `${formData.proxyUsername}@` : '';

    return `${typePart} · ${authPart}${hostPart}`;
  }, [formData, t]);

  return (
    <div className="rounded-md border border-border bg-background">
      <div className="flex items-center justify-between px-3 py-2">
        <div className="space-y-1">
          <Label className="text-sm">{t('connection.proxy.enableProxy')}</Label>
          {formData.useProxy && <p className="text-xs text-muted-foreground">{summary}</p>}
        </div>
        <Switch
          checked={formData.useProxy}
          onCheckedChange={checked => {
            onChange('useProxy', checked);
            if (checked) setIsOpen(true);
          }}
        />
      </div>

      {formData.useProxy && (
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
              {t('connection.proxy.configure')}
            </Button>
          </div>

          {isOpen && (
            <div className="px-4 pb-4 space-y-4">
              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">
                  {t('connection.proxy.type')}
                </Label>
                <Select
                  value={formData.proxyType}
                  onValueChange={value =>
                    onChange('proxyType', value as ConnectionFormData['proxyType'])
                  }
                >
                  <SelectTrigger className="h-9 w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="socks5">SOCKS5</SelectItem>
                    <SelectItem value="http_connect">HTTP CONNECT</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="grid grid-cols-3 gap-3">
                <div className="col-span-2 space-y-2">
                  <Label className="text-xs text-muted-foreground">
                    {t('connection.proxy.host')}
                  </Label>
                  <Input
                    placeholder="proxy.corp.local"
                    value={formData.proxyHost}
                    onChange={e => onChange('proxyHost', e.target.value)}
                  />
                </div>
                <div className="space-y-2">
                  <Label className="text-xs text-muted-foreground">
                    {t('connection.proxy.port')}
                  </Label>
                  <Input
                    type="number"
                    value={formData.proxyPort}
                    onChange={e => onChange('proxyPort', parseIntOr(e.target.value, 1080))}
                  />
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
                  onClick={() => setShowAuth(v => !v)}
                >
                  <span className="flex items-center gap-2">
                    {showAuth ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
                    {t('connection.proxy.authentication')}
                  </span>
                </Button>

                {showAuth && (
                  <div className="px-3 pb-3 space-y-3 border-t border-border">
                    <div className="grid grid-cols-2 gap-3 pt-3">
                      <div className="space-y-2">
                        <Label className="text-xs text-muted-foreground">
                          {t('connection.proxy.username')}
                        </Label>
                        <Input
                          placeholder={t('connection.proxy.usernameOptional')}
                          value={formData.proxyUsername}
                          onChange={e => onChange('proxyUsername', e.target.value)}
                        />
                      </div>
                      <div className="space-y-2">
                        <Label className="text-xs text-muted-foreground">
                          {t('connection.proxy.password')}
                        </Label>
                        <Input
                          type="password"
                          placeholder={t('connection.proxy.passwordOptional')}
                          value={formData.proxyPassword}
                          onChange={e => onChange('proxyPassword', e.target.value)}
                        />
                      </div>
                    </div>
                    <div className="space-y-2">
                      <Label className="text-xs text-muted-foreground">
                        {t('connection.proxy.connectTimeoutSecs')}
                      </Label>
                      <Input
                        type="number"
                        min={1}
                        value={formData.proxyConnectTimeoutSecs}
                        onChange={e =>
                          onChange('proxyConnectTimeoutSecs', parseIntOr(e.target.value, 10))
                        }
                      />
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
