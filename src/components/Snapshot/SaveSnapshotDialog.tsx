// SPDX-License-Identifier: Apache-2.0

import { Camera, Loader2 } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { notify } from '@/lib/notify';
import type { Namespace, QueryResult } from '@/lib/tauri';
import { saveSnapshot } from '@/lib/tauri';

interface SaveSnapshotDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  result: QueryResult;
  source: string;
  sourceType: 'query' | 'table';
  connectionName?: string;
  driver?: string;
  namespace?: Namespace;
  defaultName?: string;
}

export function SaveSnapshotDialog({
  open,
  onOpenChange,
  result,
  source,
  sourceType,
  connectionName,
  driver,
  namespace,
  defaultName,
}: SaveSnapshotDialogProps) {
  const { t } = useTranslation();
  const [name, setName] = useState(defaultName ?? '');
  const [description, setDescription] = useState('');
  const [saving, setSaving] = useState(false);

  const handleSave = async () => {
    if (!name.trim()) return;
    setSaving(true);
    try {
      const response = await saveSnapshot({
        name: name.trim(),
        description: description.trim() || undefined,
        source,
        source_type: sourceType,
        connection_name: connectionName,
        driver,
        namespace,
        result,
      });
      if (response.success) {
        notify.success(t('snapshots.saveSuccess'));
        onOpenChange(false);
        setName('');
        setDescription('');
      } else {
        notify.error(response.error ?? t('snapshots.saveError'));
      }
    } catch {
      notify.error(t('snapshots.saveError'));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Camera size={18} />
            {t('snapshots.save')}
          </DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-4 py-2">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="snapshot-name" className="text-sm">
              {t('snapshots.name')}
            </Label>
            <Input
              id="snapshot-name"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder={t('snapshots.namePlaceholder')}
              autoFocus
              onKeyDown={e => {
                if (e.key === 'Enter' && name.trim()) handleSave();
              }}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="snapshot-desc" className="text-sm">
              {t('snapshots.description')}
            </Label>
            <Textarea
              id="snapshot-desc"
              value={description}
              onChange={e => setDescription(e.target.value)}
              placeholder={t('snapshots.descriptionPlaceholder')}
              className="min-h-[60px] resize-y"
            />
          </div>
          <div className="rounded-md border border-border bg-muted/30 p-3 text-xs text-muted-foreground space-y-1">
            <div>
              <span className="font-medium text-foreground">{t('snapshots.source')}:</span>{' '}
              <span className="font-mono">{source.length > 80 ? `${source.slice(0, 80)}...` : source}</span>
            </div>
            <div>
              {t('snapshots.rows', { count: result.rows.length })} &middot;{' '}
              {result.columns.length} {t('snapshots.columns').toLowerCase()}
            </div>
            {connectionName && (
              <div>
                {t('snapshots.connection')}: {connectionName}
              </div>
            )}
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
            {t('common.cancel')}
          </Button>
          <Button onClick={handleSave} disabled={saving || !name.trim()}>
            {saving ? (
              <>
                <Loader2 size={14} className="mr-2 animate-spin" />
                {t('snapshots.saving')}
              </>
            ) : (
              <>
                <Camera size={14} className="mr-2" />
                {t('snapshots.save')}
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
