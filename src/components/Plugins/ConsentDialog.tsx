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
import { type InstalledPlugin, type PluginCapabilityKind, setPluginConsent } from '@/lib/plugins';

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
const ORDERED: PluginCapabilityKind[] = [
  'log',
  'notify',
  'storage',
  'queryRead',
  'http',
  'fs',
  'secrets',
];

function isRequested(
  caps: NonNullable<InstalledPlugin['manifest']['runtime']>['capabilities'],
  kind: PluginCapabilityKind
): boolean {
  if (!caps) return false;
  switch (kind) {
    case 'log':
    case 'notify':
    case 'storage':
    case 'queryRead':
      return caps[kind] === true;
    case 'http':
      return Boolean(caps.http && caps.http.allowedHosts.length > 0);
    case 'fs':
      return Boolean(caps.fs);
    case 'secrets':
      return Boolean(caps.secrets && caps.secrets.length > 0);
  }
}

/** Capabilities a manifest actually asks for, in stable UI order. Exported
 *  so callers can decide whether to even open the consent dialog. */
export function requestedCaps(plugin: InstalledPlugin | null): PluginCapabilityKind[] {
  const caps = plugin?.manifest.runtime?.capabilities;
  if (!caps) return [];
  return ORDERED.filter(k => isRequested(caps, k));
}

/** Manifest specifics surfaced below the generic description: hosts the
 *  plugin will reach, secret names it will read, etc. */
function capDetail(plugin: InstalledPlugin, cap: PluginCapabilityKind): string | null {
  const caps = plugin.manifest.runtime?.capabilities;
  if (!caps) return null;
  switch (cap) {
    case 'http':
      return caps.http && caps.http.allowedHosts.length > 0
        ? caps.http.allowedHosts.join(', ')
        : null;
    case 'secrets':
      return caps.secrets && caps.secrets.length > 0 ? caps.secrets.join(', ') : null;
    default:
      return null;
  }
}

/** True when the manifest requests the SSRF escape hatch for this capability. */
function wantsPrivateNetworks(plugin: InstalledPlugin, cap: PluginCapabilityKind): boolean {
  return (
    cap === 'http' && plugin.manifest.runtime?.capabilities?.http?.allowPrivateNetworks === true
  );
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
    setGrants(new Set(initialGrants ?? requestedCaps(plugin)));
  }, [open, plugin, initialGrants]);

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
          {requested.map(cap => {
            const detail = capDetail(plugin, cap);
            return (
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
                  {detail && (
                    <div className="mt-1 font-mono text-[10.5px] text-foreground/80">{detail}</div>
                  )}
                  {wantsPrivateNetworks(plugin, cap) && (
                    <div className="mt-1 rounded border border-warning/40 bg-warning/10 px-1.5 py-1 text-[10.5px] text-warning">
                      {t('plugins.consent.privateNetworksWarning')}
                    </div>
                  )}
                </label>
              </li>
            );
          })}
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
