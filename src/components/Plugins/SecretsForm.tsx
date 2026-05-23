// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';
import { Check, Eye, EyeOff, Save, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { InstalledPlugin } from '@/lib/plugins';

interface SecretsFormProps {
  plugin: InstalledPlugin;
}

/** Provisions the named secrets a plugin's manifest declares. Values never
 *  leave the backend after they're stored — the UI only knows whether a
 *  secret is provisioned, not what it is. */
export function SecretsForm({ plugin }: SecretsFormProps) {
  const { t } = useTranslation();
  const names = plugin.manifest.runtime?.capabilities?.secrets ?? [];
  const [provisioned, setProvisioned] = useState<Set<string>>(new Set());
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [shown, setShown] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<string[]>('list_provisioned_secrets', {
        pluginId: plugin.manifest.id,
      });
      setProvisioned(new Set(list));
    } catch {
      setProvisioned(new Set());
    }
  }, [plugin.manifest.id]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function save(name: string) {
    const value = drafts[name] ?? '';
    if (!value) return;
    setBusy(name);
    try {
      await invoke('set_plugin_secret', {
        pluginId: plugin.manifest.id,
        name,
        value,
      });
      setDrafts(d => ({ ...d, [name]: '' }));
      await refresh();
      toast.success(t('plugins.secrets.saved'));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function remove(name: string) {
    setBusy(name);
    try {
      await invoke('delete_plugin_secret', {
        pluginId: plugin.manifest.id,
        name,
      });
      await refresh();
      toast.success(t('plugins.secrets.removed'));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  if (names.length === 0) return null;

  return (
    <section>
      <h4 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
        {t('plugins.detail.secrets')}
      </h4>
      <p className="mb-2 text-xs text-muted-foreground">
        {t('plugins.detail.secretsDescription')}
      </p>
      <ul className="space-y-2">
        {names.map(name => {
          const isSet = provisioned.has(name);
          const isShown = shown.has(name);
          return (
            <li key={name} className="rounded border border-border p-2">
              <div className="mb-1.5 flex items-center justify-between text-xs">
                <span className="font-mono font-medium text-foreground">{name}</span>
                {isSet && (
                  <span className="inline-flex items-center gap-1 text-success">
                    <Check size={11} />
                    {t('plugins.secrets.provisioned')}
                  </span>
                )}
              </div>
              <div className="flex items-center gap-1.5">
                <Input
                  type={isShown ? 'text' : 'password'}
                  value={drafts[name] ?? ''}
                  onChange={e => setDrafts(d => ({ ...d, [name]: e.target.value }))}
                  placeholder={
                    isSet
                      ? t('plugins.secrets.replacePlaceholder')
                      : t('plugins.secrets.placeholder')
                  }
                  disabled={busy !== null}
                  className="h-7 text-xs"
                />
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-7 w-7 shrink-0 p-0"
                  onClick={() => {
                    const next = new Set(shown);
                    if (isShown) next.delete(name);
                    else next.add(name);
                    setShown(next);
                  }}
                  aria-label={t(isShown ? 'plugins.secrets.hide' : 'plugins.secrets.show')}
                >
                  {isShown ? <EyeOff size={12} /> : <Eye size={12} />}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  className="h-7 shrink-0 gap-1 px-2 text-xs"
                  disabled={busy !== null || !drafts[name]}
                  onClick={() => save(name)}
                >
                  <Save size={11} />
                  {t('plugins.secrets.save')}
                </Button>
                {isSet && (
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-7 w-7 shrink-0 p-0 text-muted-foreground hover:text-destructive"
                    disabled={busy !== null}
                    onClick={() => remove(name)}
                    aria-label={t('plugins.secrets.remove')}
                  >
                    <Trash2 size={12} />
                  </Button>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
