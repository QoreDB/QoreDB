// SPDX-License-Identifier: Apache-2.0

import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { FolderOpen } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { installPlugin, type InstalledPlugin } from '@/lib/plugins';
import { ConsentDialog } from './ConsentDialog';

interface InstallPluginDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onInstalled: () => void;
}

export function InstallPluginDialog({ open, onOpenChange, onInstalled }: InstallPluginDialogProps) {
  const { t } = useTranslation();
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Plugin that just installed and requests capabilities — chained into
   *  the consent dialog so the user makes the trust call up front. */
  const [pendingConsent, setPendingConsent] = useState<InstalledPlugin | null>(null);

  async function pickAndInstall() {
    setError(null);
    let folder: string | null;
    try {
      const picked = await openDialog({ directory: true, multiple: false });
      folder = Array.isArray(picked) ? (picked[0] ?? null) : picked;
    } catch {
      return;
    }
    if (!folder) return;

    setInstalling(true);
    try {
      const plugin = await installPlugin(folder);
      const c = plugin.manifest.contributes;
      const contributions: string[] = [];
      if (c.snippets.length) contributions.push('snippets');
      if (c.connectionTemplates.length) contributions.push('connectionTemplates');
      if (c.themes.length) contributions.push('themes');
      AnalyticsService.capture('plugin_installed', { contributions });
      toast.success(t('plugins.toast.installed', { name: plugin.manifest.name }));

      const caps = plugin.manifest.runtime?.capabilities;
      const wantsConsent =
        caps && (caps.log || caps.notify || caps.storage || caps.queryRead);
      if (wantsConsent) {
        setPendingConsent(plugin);
        onOpenChange(false);
      } else {
        onInstalled();
        onOpenChange(false);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setInstalling(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={o => !installing && onOpenChange(o)}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('plugins.installDialog.title')}</DialogTitle>
          <DialogDescription>{t('plugins.installDialog.description')}</DialogDescription>
        </DialogHeader>

        {error && (
          <p className="rounded border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {error}
          </p>
        )}

        <DialogFooter>
          <Button variant="ghost" disabled={installing} onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button disabled={installing} onClick={pickAndInstall} className="gap-1.5">
            <FolderOpen size={14} />
            {installing
              ? t('plugins.installDialog.installing')
              : t('plugins.installDialog.pickFolder')}
          </Button>
        </DialogFooter>
      </DialogContent>

      <ConsentDialog
        plugin={pendingConsent}
        open={pendingConsent !== null}
        onOpenChange={o => {
          if (!o) {
            setPendingConsent(null);
            onInstalled();
          }
        }}
        onSaved={() => {
          setPendingConsent(null);
          onInstalled();
        }}
      />
    </Dialog>
  );
}
