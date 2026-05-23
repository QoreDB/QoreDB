// SPDX-License-Identifier: Apache-2.0

import { Shield } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  type InstalledPlugin,
  type PluginCapabilityKind,
  setPluginConsent,
} from '@/lib/plugins';

interface ConsentDialogProps {
  plugin: InstalledPlugin | null;
  /** Capabilities currently granted, used to prefill the checkboxes when the
   *  dialog is opened from the detail view. Defaults to "all requested" the
   *  first time. */
  initialGrants?: PluginCapabilityKind[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
}

/** Capabilities a manifest can request, in stable UI order. */
const ORDERED: PluginCapabilityKind[] = ['log', 'notify', 'storage', 'queryRead'];

function requestedCaps(plugin: InstalledPlugin | null): PluginCapabilityKind[] {
  const caps = plugin?.manifest.runtime?.capabilities;
  if (!caps) return [];
  return ORDERED.filter(k => caps[k]);
}

export function ConsentDialog({
  plugin,
  initialGrants,
  open,
  onOpenChange,
  onSaved,
}: ConsentDialogProps) {
  const { t } = useTranslation();
  const requested = requestedCaps(plugin);
  const [grants, setGrants] = useState<Set<PluginCapabilityKind>>(new Set());
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open || !plugin) return;
    setGrants(new Set(initialGrants ?? requested));
  }, [open, plugin, initialGrants, requested]);

  if (!plugin || requested.length === 0) return null;

  async function save(toGrant: PluginCapabilityKind[]) {
    if (!plugin) return;
    setSaving(true);
    try {
      await setPluginConsent(plugin.manifest.id, toGrant);
      onSaved();
      onOpenChange(false);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  function toggle(cap: PluginCapabilityKind, checked: boolean) {
    const next = new Set(grants);
    if (checked) next.add(cap);
    else next.delete(cap);
    setGrants(next);
  }

  return (
    <Dialog open={open} onOpenChange={o => !saving && onOpenChange(o)}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Shield size={16} className="text-warning" />
            {t('plugins.consent.title', { name: plugin.manifest.name })}
          </DialogTitle>
          <DialogDescription>{t('plugins.consent.description')}</DialogDescription>
        </DialogHeader>

        <ul className="space-y-2">
          {requested.map(cap => (
            <li
              key={cap}
              className="flex items-start gap-3 rounded-lg border border-border p-2.5"
            >
              <Checkbox
                id={`cap-${cap}`}
                checked={grants.has(cap)}
                onCheckedChange={c => toggle(cap, c === true)}
                disabled={saving}
                className="mt-0.5"
              />
              <label htmlFor={`cap-${cap}`} className="min-w-0 flex-1 cursor-pointer">
                <div className="text-sm font-medium text-foreground">
                  {t(`plugins.consent.caps.${cap}.title`)}
                </div>
                <div className="text-xs text-muted-foreground">
                  {t(`plugins.consent.caps.${cap}.description`)}
                </div>
              </label>
            </li>
          ))}
        </ul>

        <DialogFooter>
          <Button variant="ghost" disabled={saving} onClick={() => save([])}>
            {t('plugins.consent.denyAll')}
          </Button>
          <Button disabled={saving} onClick={() => save(Array.from(grants))}>
            {saving ? t('plugins.consent.saving') : t('plugins.consent.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
