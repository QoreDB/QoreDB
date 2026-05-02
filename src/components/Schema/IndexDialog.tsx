// SPDX-License-Identifier: Apache-2.0

import { Loader2, Plus, Trash2 } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { DangerConfirmDialog } from '@/components/Guard/DangerConfirmDialog';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import { executeQuery } from '@/lib/tauri';

type IndexDirection = '1' | '-1' | 'text' | '2dsphere';

interface KeyRow {
  field: string;
  direction: IndexDirection;
}

interface IndexDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
  sessionId: string;
  database: string;
  collection: string;
  fieldSuggestions?: string[];
  environment?: 'development' | 'staging' | 'production';
}

const DIRECTIONS: { value: IndexDirection; labelKey: string }[] = [
  { value: '1', labelKey: 'mongoIndex.directionAsc' },
  { value: '-1', labelKey: 'mongoIndex.directionDesc' },
  { value: 'text', labelKey: 'mongoIndex.directionText' },
  { value: '2dsphere', labelKey: 'mongoIndex.direction2dsphere' },
];

export function IndexDialog({
  isOpen,
  onClose,
  onSuccess,
  sessionId,
  database,
  collection,
  fieldSuggestions = [],
  environment = 'development',
}: IndexDialogProps) {
  const { t } = useTranslation();
  const [keys, setKeys] = useState<KeyRow[]>([{ field: '', direction: '1' }]);
  const [name, setName] = useState('');
  const [unique, setUnique] = useState(false);
  const [sparse, setSparse] = useState(false);
  const [ttlEnabled, setTtlEnabled] = useState(false);
  const [ttlSeconds, setTtlSeconds] = useState('');
  const [partialFilter, setPartialFilter] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);

  useEffect(() => {
    if (isOpen) {
      setKeys([{ field: '', direction: '1' }]);
      setName('');
      setUnique(false);
      setSparse(false);
      setTtlEnabled(false);
      setTtlSeconds('');
      setPartialFilter('');
      setError(null);
    }
  }, [isOpen]);

  const payload = useMemo(() => {
    const keysObj: Record<string, number | string> = {};
    for (const k of keys) {
      if (!k.field.trim()) continue;
      if (k.direction === '1' || k.direction === '-1') {
        keysObj[k.field.trim()] = Number(k.direction);
      } else {
        keysObj[k.field.trim()] = k.direction;
      }
    }
    const options: Record<string, unknown> = {};
    if (name.trim()) options.name = name.trim();
    if (unique) options.unique = true;
    if (sparse) options.sparse = true;
    if (ttlEnabled && ttlSeconds.trim()) {
      const n = Number(ttlSeconds);
      if (Number.isFinite(n) && n >= 0) options.expireAfterSeconds = n;
    }
    if (partialFilter.trim()) {
      try {
        options.partialFilterExpression = JSON.parse(partialFilter);
      } catch {
        // keep invalid → validated at submit
      }
    }
    return {
      operation: 'createIndex',
      database,
      collection,
      keys: keysObj,
      ...(Object.keys(options).length > 0 ? { options } : {}),
    };
  }, [keys, name, unique, sparse, ttlEnabled, ttlSeconds, partialFilter, database, collection]);

  const preview = useMemo(() => JSON.stringify(payload, null, 2), [payload]);

  function validate(): string | null {
    const fields = keys.filter(k => k.field.trim());
    if (fields.length === 0) return t('mongoIndex.errorNoField');
    const seen = new Set<string>();
    for (const k of fields) {
      if (seen.has(k.field)) return t('mongoIndex.errorDuplicateField', { field: k.field });
      seen.add(k.field);
    }
    if (ttlEnabled) {
      const n = Number(ttlSeconds);
      if (!Number.isFinite(n) || n < 0) return t('mongoIndex.errorTtl');
      if (fields.length !== 1 || (fields[0]?.direction !== '1' && fields[0]?.direction !== '-1')) {
        return t('mongoIndex.errorTtlSingleField');
      }
    }
    if (partialFilter.trim()) {
      try {
        const parsed = JSON.parse(partialFilter);
        if (typeof parsed !== 'object' || Array.isArray(parsed) || parsed === null) {
          return t('mongoIndex.errorPartialFilterShape');
        }
      } catch {
        return t('mongoIndex.errorPartialFilterJson');
      }
    }
    return null;
  }

  function addKey() {
    setKeys(prev => [...prev, { field: '', direction: '1' }]);
  }

  function removeKey(idx: number) {
    setKeys(prev => prev.filter((_, i) => i !== idx));
  }

  function updateKey(idx: number, patch: Partial<KeyRow>) {
    setKeys(prev => prev.map((k, i) => (i === idx ? { ...k, ...patch } : k)));
  }

  function handleSubmit() {
    const err = validate();
    if (err) {
      setError(err);
      return;
    }
    setError(null);
    if (environment !== 'development') {
      setConfirmOpen(true);
      return;
    }
    void runCreate(false);
  }

  async function runCreate(acknowledgedDangerous: boolean) {
    setLoading(true);
    setError(null);
    try {
      const res = await executeQuery(sessionId, JSON.stringify(payload), {
        acknowledgedDangerous,
      });
      if (res.success) {
        toast.success(t('mongoIndex.created'));
        onSuccess();
        onClose();
      } else {
        setError(res.error ?? t('common.unknownError'));
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : t('common.unknownError'));
    } finally {
      setLoading(false);
      setConfirmOpen(false);
    }
  }

  return (
    <>
      <Dialog open={isOpen} onOpenChange={open => !open && !loading && onClose()}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t('mongoIndex.createTitle')}</DialogTitle>
          </DialogHeader>

          <div className="space-y-4 max-h-[60vh] overflow-y-auto pr-1">
            <div className="text-xs text-muted-foreground">
              <span className="font-mono">{database}</span> /{' '}
              <span className="font-mono">{collection}</span>
            </div>

            <div className="space-y-2">
              <Label>{t('mongoIndex.keysLabel')}</Label>
              <div className="space-y-2">
                {keys.map((k, idx) => (
                  <div key={idx} className="flex gap-2 items-center">
                    <Input
                      placeholder={t('mongoIndex.fieldPlaceholder')}
                      value={k.field}
                      onChange={e => updateKey(idx, { field: e.target.value })}
                      list={fieldSuggestions.length > 0 ? `mongo-index-fields-${idx}` : undefined}
                      className="flex-1"
                    />
                    {fieldSuggestions.length > 0 && (
                      <datalist id={`mongo-index-fields-${idx}`}>
                        {fieldSuggestions.map(f => (
                          <option key={f} value={f} />
                        ))}
                      </datalist>
                    )}
                    <Select
                      value={k.direction}
                      onValueChange={v => updateKey(idx, { direction: v as IndexDirection })}
                    >
                      <SelectTrigger className="w-40">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {DIRECTIONS.map(d => (
                          <SelectItem key={d.value} value={d.value}>
                            {t(d.labelKey)}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => removeKey(idx)}
                      disabled={keys.length === 1}
                      aria-label={t('mongoIndex.removeKey')}
                    >
                      <Trash2 size={16} />
                    </Button>
                  </div>
                ))}
              </div>
              <Button variant="ghost" size="sm" onClick={addKey} className="gap-2">
                <Plus size={14} /> {t('mongoIndex.addKey')}
              </Button>
            </div>

            <div className="space-y-2">
              <Label htmlFor="mongo-index-name">{t('mongoIndex.nameLabel')}</Label>
              <Input
                id="mongo-index-name"
                value={name}
                onChange={e => setName(e.target.value)}
                placeholder={t('mongoIndex.namePlaceholder')}
              />
              <p className="text-xs text-muted-foreground">{t('mongoIndex.nameHint')}</p>
            </div>

            <div className="space-y-2">
              <Label>{t('mongoIndex.optionsLabel')}</Label>
              <div className="flex items-center gap-2">
                <Checkbox
                  id="mongo-index-unique"
                  checked={unique}
                  onCheckedChange={v => setUnique(v === true)}
                />
                <Label htmlFor="mongo-index-unique" className="font-normal cursor-pointer">
                  {t('mongoIndex.unique')}
                </Label>
              </div>
              <div className="flex items-center gap-2">
                <Checkbox
                  id="mongo-index-sparse"
                  checked={sparse}
                  onCheckedChange={v => setSparse(v === true)}
                />
                <Label htmlFor="mongo-index-sparse" className="font-normal cursor-pointer">
                  {t('mongoIndex.sparse')}
                </Label>
              </div>
              <div className="flex items-center gap-2">
                <Checkbox
                  id="mongo-index-ttl"
                  checked={ttlEnabled}
                  onCheckedChange={v => setTtlEnabled(v === true)}
                />
                <Label htmlFor="mongo-index-ttl" className="font-normal cursor-pointer">
                  {t('mongoIndex.ttl')}
                </Label>
                <Input
                  type="number"
                  min="0"
                  value={ttlSeconds}
                  onChange={e => setTtlSeconds(e.target.value)}
                  disabled={!ttlEnabled}
                  placeholder={t('mongoIndex.ttlPlaceholder')}
                  className="w-32 ml-2"
                />
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="mongo-index-pfe">{t('mongoIndex.partialFilter')}</Label>
              <Textarea
                id="mongo-index-pfe"
                value={partialFilter}
                onChange={e => setPartialFilter(e.target.value)}
                placeholder='{"status": {"$ne": "deleted"}}'
                rows={3}
                className="font-mono text-xs"
              />
              <p className="text-xs text-muted-foreground">{t('mongoIndex.partialFilterHint')}</p>
            </div>

            <div className="space-y-2">
              <Label>{t('mongoIndex.preview')}</Label>
              <pre className="rounded-md border border-border bg-muted/50 p-3 text-xs font-mono overflow-x-auto">
                {preview}
              </pre>
            </div>

            {error && (
              <div className="rounded-md bg-error/10 border border-error/20 text-error text-sm px-3 py-2">
                {error}
              </div>
            )}
          </div>

          <DialogFooter>
            <Button variant="ghost" onClick={onClose} disabled={loading}>
              {t('common.cancel')}
            </Button>
            <Button onClick={handleSubmit} disabled={loading} className="gap-2">
              {loading && <Loader2 size={14} className="animate-spin" />}
              {t('mongoIndex.create')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <DangerConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t('mongoIndex.confirmCreateTitle')}
        description={t('mongoIndex.confirmCreateDesc', { database, collection })}
        confirmLabel={t('mongoIndex.create')}
        confirmationLabel={environment === 'production' ? collection : undefined}
        loading={loading}
        onConfirm={() => {
          void runCreate(true);
        }}
      />
    </>
  );
}
