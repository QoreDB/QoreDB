// SPDX-License-Identifier: Apache-2.0

import { Pencil } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  getPluginConsent,
  type InstalledPlugin,
  type PluginCapabilityKind,
} from '@/lib/plugins';
import { ConsentDialog } from './ConsentDialog';

interface PluginDetailDialogProps {
  plugin: InstalledPlugin | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Notified when the user updates consent so the parent can refresh. */
  onConsentChanged?: () => void;
}

const CAP_ORDER: PluginCapabilityKind[] = ['log', 'notify', 'storage', 'queryRead'];

export function PluginDetailDialog({
  plugin,
  open,
  onOpenChange,
  onConsentChanged,
}: PluginDetailDialogProps) {
  const { t } = useTranslation();
  const [grants, setGrants] = useState<PluginCapabilityKind[]>([]);
  const [editing, setEditing] = useState(false);

  const refreshGrants = useCallback(async () => {
    if (!plugin) return;
    try {
      setGrants(await getPluginConsent(plugin.manifest.id));
    } catch {
      setGrants([]);
    }
  }, [plugin]);

  useEffect(() => {
    if (open && plugin) {
      refreshGrants();
    }
  }, [open, plugin, refreshGrants]);

  if (!plugin) return null;

  const { manifest } = plugin;
  const c = manifest.contributes;
  const hasContributions = c.snippets.length + c.connectionTemplates.length + c.themes.length > 0;

  const requested = CAP_ORDER.filter(k => manifest.runtime?.capabilities?.[k]);
  const grantedSet = new Set(grants);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{manifest.name}</DialogTitle>
          <DialogDescription>
            {t('plugins.card.version')} {manifest.version}
            {manifest.author ? ` · ${t('plugins.card.by', { author: manifest.author })}` : ''}
          </DialogDescription>
        </DialogHeader>

        <div className="max-h-[60vh] space-y-4 overflow-auto text-sm">
          {manifest.description && <p className="text-muted-foreground">{manifest.description}</p>}
          {manifest.qoredb && (
            <p className="text-xs text-muted-foreground">
              {t('plugins.detail.requires', { version: manifest.qoredb })}
            </p>
          )}

          {manifest.runtime && (
            <section className="rounded-lg border border-warning/30 bg-warning/5 p-2.5">
              <h4 className="mb-1 text-xs font-semibold uppercase tracking-wide text-warning">
                {t('plugins.detail.runtime')}
              </h4>
              <p className="text-xs text-muted-foreground">
                {t('plugins.detail.runtimeExplanation')}
              </p>
              <dl className="mt-1.5 space-y-0.5 text-xs">
                <div className="flex gap-1.5">
                  <dt className="text-muted-foreground">{t('plugins.detail.runtimeEntry')}</dt>
                  <dd className="font-mono text-foreground">{manifest.runtime.entry}</dd>
                </div>
                <div className="flex gap-1.5">
                  <dt className="text-muted-foreground">{t('plugins.detail.runtimeAbi')}</dt>
                  <dd className="text-foreground">v{manifest.runtime.abiVersion}</dd>
                </div>
                {manifest.runtime.hooks.length > 0 && (
                  <div className="flex gap-1.5">
                    <dt className="text-muted-foreground">{t('plugins.detail.runtimeHooks')}</dt>
                    <dd className="font-mono text-foreground">
                      {manifest.runtime.hooks.join(', ')}
                    </dd>
                  </div>
                )}
              </dl>
            </section>
          )}

          {requested.length > 0 && (
            <section>
              <div className="mb-1 flex items-center justify-between">
                <h4 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                  {t('plugins.detail.capabilities')}
                </h4>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-6 gap-1 px-2 text-xs"
                  onClick={() => setEditing(true)}
                >
                  <Pencil size={11} />
                  {t('plugins.detail.editConsent')}
                </Button>
              </div>
              <ul className="space-y-1">
                {requested.map(cap => {
                  const granted = grantedSet.has(cap);
                  return (
                    <li
                      key={cap}
                      className="flex items-center justify-between rounded border border-border px-2 py-1 text-xs"
                    >
                      <span className="font-medium text-foreground">
                        {t(`plugins.consent.caps.${cap}.title`)}
                      </span>
                      <span
                        className={
                          granted
                            ? 'text-success'
                            : 'text-muted-foreground'
                        }
                      >
                        {t(granted ? 'plugins.detail.granted' : 'plugins.detail.notGranted')}
                      </span>
                    </li>
                  );
                })}
              </ul>
            </section>
          )}

          {!hasContributions && !manifest.runtime && (
            <p className="text-xs text-muted-foreground">{t('plugins.detail.noContributions')}</p>
          )}

          {c.snippets.length > 0 && (
            <section>
              <h4 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                {t('plugins.detail.snippets')}
              </h4>
              <ul className="space-y-1">
                {c.snippets.map(s => (
                  <li key={s.id} className="rounded border border-border px-2 py-1">
                    <span className="font-medium text-foreground">{s.label}</span>
                    {s.description && (
                      <span className="text-xs text-muted-foreground"> — {s.description}</span>
                    )}
                  </li>
                ))}
              </ul>
            </section>
          )}

          {c.connectionTemplates.length > 0 && (
            <section>
              <h4 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                {t('plugins.detail.templates')}
              </h4>
              <ul className="space-y-1">
                {c.connectionTemplates.map(tpl => (
                  <li key={tpl.id} className="rounded border border-border px-2 py-1">
                    <span className="font-medium text-foreground">{tpl.name}</span>
                    <span className="text-xs text-muted-foreground"> — {tpl.driver}</span>
                  </li>
                ))}
              </ul>
            </section>
          )}

          {c.themes.length > 0 && (
            <section>
              <h4 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                {t('plugins.detail.themes')}
              </h4>
              <ul className="space-y-1">
                {c.themes.map(th => (
                  <li key={th.id} className="rounded border border-border px-2 py-1">
                    <span className="font-medium text-foreground">{th.name}</span>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </div>
      </DialogContent>

      <ConsentDialog
        plugin={editing ? plugin : null}
        initialGrants={grants}
        open={editing}
        onOpenChange={setEditing}
        onSaved={() => {
          setEditing(false);
          refreshGrants();
          onConsentChanged?.();
        }}
      />
    </Dialog>
  );
}
