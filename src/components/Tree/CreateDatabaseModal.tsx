import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { createDatabase, Environment } from '../../lib/tauri';
import { toast } from 'sonner';
import { Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { getDriverMetadata } from '../../lib/drivers';
import { isDocumentDatabase } from '../../lib/driverCapabilities';
import { ProductionConfirmDialog } from '../Guard/ProductionConfirmDialog';

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

  const driverMeta = getDriverMetadata(driver);
  const isDocument = isDocumentDatabase(driver);
  const confirmationLabel = (connectionDatabase || connectionName || 'PROD').trim() || 'PROD';

  useEffect(() => {
    if (!isOpen) return;
    setName('');
    setCollectionName('');
  }, [isOpen]);

  async function performCreate(acknowledgedDangerous: boolean) {
    setLoading(true);
    try {
      let options = undefined;
      if (isDocument) {
        options = { collection: collectionName.trim() };
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
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t(titleKey)}</DialogTitle>
          </DialogHeader>

          <div className="space-y-3 py-4">
            <div className="space-y-2">
              <label className="text-sm font-medium">{t(nameLabelKey)}</label>
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
                  <label className="text-sm font-medium">{t('database.collectionNameLabel')}</label>
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
