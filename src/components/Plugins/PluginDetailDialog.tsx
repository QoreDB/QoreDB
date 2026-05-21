// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { InstalledPlugin } from '@/lib/plugins';

interface PluginDetailDialogProps {
  plugin: InstalledPlugin | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function PluginDetailDialog({ plugin, open, onOpenChange }: PluginDetailDialogProps) {
  const { t } = useTranslation();
  if (!plugin) return null;

  const { manifest } = plugin;
  const c = manifest.contributes;
  const hasContributions = c.snippets.length + c.connectionTemplates.length + c.themes.length > 0;

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

          {!hasContributions && (
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
    </Dialog>
  );
}
