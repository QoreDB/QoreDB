// SPDX-License-Identifier: Apache-2.0

import { Loader2 } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { isDocumentDatabase } from '../../lib/driverCapabilities';
import { getDriverMetadata } from '../../lib/drivers';
import {
  type CharsetInfo,
  createDatabase,
  type Environment,
  getCreationOptions,
} from '../../lib/tauri';
import { ProductionConfirmDialog } from '../Guard/ProductionConfirmDialog';
import { Label } from '../ui/label';

interface CreateDatabaseModalProps {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string;
  driver: string;
  environment?: Environment;
  readOnly?: boolean;
  connectionName?: string;
  connectionDatabase?: string;
  onCreated: () => void;
}

export function CreateDatabaseModal({
  isOpen,
  onClose,
  sessionId,
  driver,
  environment = 'development',
  readOnly = false,
  connectionName,
  connectionDatabase,
  onCreated,
}: CreateDatabaseModalProps) {
  const { t } = useTranslation();
  const [name, setName] = useState('');
  const [collectionName, setCollectionName] = useState('');
  const [loading, setLoading] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [pendingAction, setPendingAction] = useState<null | (() => Promise<void>)>(null);

  // MySQL-specific state
  const [creationOptions, setCreationOptions] = useState<CharsetInfo[]>([]);
  const [loadingOptions, setLoadingOptions] = useState(false);
  const [charset, setCharset] = useState('');
  const [collation, setCollation] = useState('');

  const driverMeta = getDriverMetadata(driver);
  const isDocument = isDocumentDatabase(driver);
  const isMysql = driver === 'mysql';
  const confirmationLabel = (connectionDatabase || connectionName || 'PROD').trim() || 'PROD';

  // Collations filtered by selected charset
  const availableCollations = useMemo(() => {
    if (!charset) return [];
    const found = creationOptions.find(cs => cs.name === charset);
    return found?.collations ?? [];
  }, [creationOptions, charset]);

  useEffect(() => {
    if (!isOpen) return;
    setName('');
    setCollectionName('');
    setCharset('');
    setCollation('');

    if (isMysql) {
      setLoadingOptions(true);
      getCreationOptions(sessionId)
        .then(res => {
          if (res.success && res.options) {
            setCreationOptions(res.options.charsets);
          }
        })
        .catch(() => {
          // silently ignore — charset/collation fields just won't appear
        })
        .finally(() => setLoadingOptions(false));
    }
  }, [isOpen, isMysql, sessionId]);

  // Reset collation when charset changes
  useEffect(() => {
    setCollation('');
  }, []);

  async function performCreate(acknowledgedDangerous: boolean) {
    setLoading(true);
    try {
      let options: Record<string, unknown> | undefined;
      if (isDocument) {
        options = { collection: collectionName.trim() };
      } else if (isMysql) {
        if (charset || collation) {
          options = {};
          if (charset) options.charset = charset;
          if (collation) options.collation = collation;
        }
      }

      const result = await createDatabase(sessionId, name.trim(), options, acknowledgedDangerous);

      if (result.success) {
        const successKey = isDocument
          ? 'database.mongoCreateSuccess'
          : driverMeta.createAction === 'schema'
            ? 'database.schemaCreateSuccess'
            : 'database.databaseCreateSuccess';
        toast.success(t(successKey));
        onCreated();
        onClose();
        setName('');
        setCollectionName('');
      } else {
        if (
          result.error &&
          (result.error.includes('1044') ||
            result.error.includes('Access denied') ||
            result.error.includes('Permission denied'))
        ) {
          toast.error(t('database.permissionDenied'), {
            description: t('database.permissionDeniedHint'),
          });
        } else {
          const errorKey = isDocument
            ? 'database.mongoCreateError'
            : driverMeta.createAction === 'schema'
              ? 'database.schemaCreateError'
              : 'database.databaseCreateError';
          toast.error(t(errorKey), {
            description: result.error,
          });
        }
      }
    } catch (err) {
      toast.error(t('common.error'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    } finally {
      setLoading(false);
    }
  }

  function handleCreate() {
    if (!name.trim()) return;
    if (isDocument && !collectionName.trim()) return;

    if (readOnly) {
      toast.error(t('environment.blocked'));
      return;
    }

    if (environment !== 'development') {
      setPendingAction(() => () => performCreate(true));
      setConfirmOpen(true);
      return;
    }

    void performCreate(false);
  }

  function handleOpenChange(open: boolean) {
    if (!open) {
      onClose();
      setName('');
      setCollectionName('');
    }
  }

  if (driverMeta.createAction === 'none') {
    return null;
  }

  const titleKey =
    driverMeta.createAction === 'schema' ? 'database.newSchema' : 'database.newDatabase';

  const nameLabelKey =
    driverMeta.createAction === 'schema'
      ? 'database.schemaNameLabel'
      : 'database.databaseNameLabel';

  return (
    <>
      <Dialog open={isOpen} onOpenChange={handleOpenChange}>
        <DialogContent className={isMysql ? 'max-w-md' : 'max-w-sm'}>
          <DialogHeader>
            <DialogTitle>{t(titleKey)}</DialogTitle>
          </DialogHeader>

          <div className="space-y-3 py-4">
            <div className="space-y-2">
              <Label className="text-sm font-medium">{t(nameLabelKey)}</Label>
              <Input
                value={name}
                onChange={e => setName(e.target.value)}
                placeholder={
                  driverMeta.createAction === 'schema'
                    ? t('database.schemaNamePlaceholder')
                    : t('database.databaseNamePlaceholder')
                }
                autoFocus
                onKeyDown={e => {
                  if (e.key === 'Enter') handleCreate();
                }}
                disabled={loading}
              />
            </div>

            {isDocument && (
              <>
                <div className="space-y-2">
                  <Label className="text-sm font-medium">{t('database.collectionNameLabel')}</Label>
                  <Input
                    value={collectionName}
                    onChange={e => setCollectionName(e.target.value)}
                    placeholder={t('database.collectionNamePlaceholder')}
                    onKeyDown={e => {
                      if (e.key === 'Enter') handleCreate();
                    }}
                    disabled={loading}
                  />
                </div>
                <p className="text-xs text-muted-foreground">{t('common.mongoCreateDbHint')}</p>
              </>
            )}

            {isMysql && !loadingOptions && creationOptions.length > 0 && (
              <>
                <div className="space-y-2">
                  <Label className="text-sm font-medium">{t('database.charsetLabel')}</Label>
                  <Select value={charset} onValueChange={setCharset} disabled={loading}>
                    <SelectTrigger>
                      <SelectValue placeholder={t('database.charsetPlaceholder')} />
                    </SelectTrigger>
                    <SelectContent>
                      {creationOptions.map(cs => (
                        <SelectItem key={cs.name} value={cs.name}>
                          {cs.name}
                          {cs.description ? ` — ${cs.description}` : ''}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>

                <div className="space-y-2">
                  <Label className="text-sm font-medium">{t('database.collationLabel')}</Label>
                  <Select
                    value={collation}
                    onValueChange={setCollation}
                    disabled={loading || !charset}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder={t('database.collationPlaceholder')} />
                    </SelectTrigger>
                    <SelectContent>
                      {availableCollations.map(col => (
                        <SelectItem key={col.name} value={col.name}>
                          {col.name}
                          {col.is_default ? ' ★' : ''}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </>
            )}

            {isMysql && loadingOptions && (
              <p className="text-xs text-muted-foreground flex items-center gap-1">
                <Loader2 className="h-3 w-3 animate-spin" />
                {t('database.loadingOptions')}
              </p>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={onClose} disabled={loading}>
              {t('common.cancel')}
            </Button>
            <Button
              onClick={handleCreate}
              disabled={loading || !name.trim() || (isDocument && !collectionName.trim())}
            >
              {loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {t('common.create')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ProductionConfirmDialog
        open={confirmOpen}
        onOpenChange={open => {
          setConfirmOpen(open);
          if (!open) {
            setPendingAction(null);
          }
        }}
        title={t('environment.confirmTitle')}
        description={t('database.confirmCreate')}
        confirmationLabel={confirmationLabel}
        confirmLabel={t('common.confirm')}
        onConfirm={() => {
          const action = pendingAction;
          setPendingAction(null);
          if (action) {
            void action();
          }
        }}
      />
    </>
  );
}
