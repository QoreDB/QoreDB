// SPDX-License-Identifier: Apache-2.0

import { HardDrive, Loader2, RefreshCw, ShieldAlert, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { formatFileSize } from '@/lib/binaryUtils';
import { notify } from '@/lib/notify';
import {
  getLocalDataUsage,
  type LocalDataCategory,
  type LocalDataUsageResponse,
  purgeLocalDataCategory,
} from '@/lib/tauri';
import { SettingsCard } from '../SettingsCard';

interface LocalDataCenterCardProps {
  searchQuery?: string;
}

export function LocalDataCenterCard({ searchQuery }: LocalDataCenterCardProps) {
  const { t } = useTranslation();
  const [usage, setUsage] = useState<LocalDataUsageResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [purgingCategory, setPurgingCategory] = useState<LocalDataCategory | null>(null);
  const [confirmCategory, setConfirmCategory] = useState<LocalDataCategory | null>(null);

  const loadUsage = useCallback(async () => {
    setLoading(true);
    try {
      const data = await getLocalDataUsage();
      setUsage(data);
    } catch {
      // Ignore in web mode or when backend is unavailable
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadUsage();
  }, [loadUsage]);

  const handleConfirmPurge = async () => {
    if (!confirmCategory) return;
    const target = confirmCategory;
    setConfirmCategory(null);
    setPurgingCategory(target);
    try {
      const updated = await purgeLocalDataCategory(target);
      setUsage(updated);
      notify.success(
        t('settings.localData.purgeSuccess', {
          category: t(`settings.localData.categories.${target}.title`),
        })
      );
    } catch (err) {
      notify.error(t('settings.localData.purgeError'), err);
    } finally {
      setPurgingCategory(null);
    }
  };

  return (
    <>
      <SettingsCard
        id="local-data-center"
        title={t('settings.localData.title')}
        description={t('settings.localData.description')}
        searchQuery={searchQuery}
      >
        <div className="space-y-4">
          <div className="flex items-start gap-2.5 rounded-md border border-border/60 bg-muted/40 p-3 text-xs text-muted-foreground">
            <ShieldAlert size={15} className="mt-0.5 shrink-0 text-warning" />
            <div className="space-y-1">
              <p>{t('settings.localData.securityNotice')}</p>
              {usage?.app_data_dir && (
                <p className="font-mono text-[11px] text-foreground/80 break-all">
                  {usage.app_data_dir}
                </p>
              )}
            </div>
          </div>

          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span className="flex items-center gap-1.5 font-medium text-foreground">
              <HardDrive size={14} />
              {t('settings.localData.totalUsage', {
                size: formatFileSize(usage?.total_bytes ?? 0),
              })}
            </span>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-7 gap-1.5 text-xs"
              disabled={loading}
              onClick={() => void loadUsage()}
            >
              <RefreshCw size={12} className={loading ? 'animate-spin' : undefined} />
              {t('common.refresh')}
            </Button>
          </div>

          <div className="divide-y divide-border/60 rounded-md border border-border/60">
            {(usage?.categories ?? []).map(item => {
              const isEmpty = item.bytes === 0 && item.file_count === 0;
              const isPurging = purgingCategory === item.category;
              return (
                <div
                  key={item.category}
                  className="flex items-center justify-between gap-4 px-3 py-2.5 text-sm"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-medium text-foreground">
                        {t(`settings.localData.categories.${item.category}.title`)}
                      </span>
                      <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
                        {formatFileSize(item.bytes)}
                      </span>
                      <span className="text-xs text-muted-foreground">
                        ({t('settings.localData.fileCount', { count: item.file_count })})
                      </span>
                    </div>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                      {t(`settings.localData.categories.${item.category}.description`)}
                    </p>
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-8 shrink-0 gap-1.5 text-xs text-destructive hover:text-destructive"
                    disabled={isEmpty || isPurging || loading}
                    onClick={() => setConfirmCategory(item.category)}
                  >
                    {isPurging ? (
                      <Loader2 size={13} className="animate-spin" />
                    ) : (
                      <Trash2 size={13} />
                    )}
                    {t('settings.localData.purge')}
                  </Button>
                </div>
              );
            })}
          </div>
        </div>
      </SettingsCard>

      <Dialog
        open={confirmCategory !== null}
        onOpenChange={open => {
          if (!open) setConfirmCategory(null);
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t('settings.localData.confirmTitle')}</DialogTitle>
            <DialogDescription>
              {confirmCategory
                ? t('settings.localData.confirmDescription', {
                    category: t(`settings.localData.categories.${confirmCategory}.title`),
                  })
                : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="gap-2 sm:justify-end">
            <Button type="button" variant="outline" onClick={() => setConfirmCategory(null)}>
              {t('common.cancel')}
            </Button>
            <Button type="button" variant="destructive" onClick={() => void handleConfirmPurge()}>
              {t('settings.localData.confirmAction')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
