import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { 
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter
} from '@/components/ui/dialog';
import { executeQuery } from '../../lib/tauri';
import { toast } from 'sonner';
import { Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { getDriverMetadata } from '../../lib/drivers';

interface CreateDatabaseModalProps {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string;
  driver: string;
  onCreated: () => void;
}

export function CreateDatabaseModal({ 
  isOpen, 
  onClose, 
  sessionId, 
  driver,
  onCreated 
}: CreateDatabaseModalProps) {
  const { t } = useTranslation();
  const [name, setName] = useState('');
  const [loading, setLoading] = useState(false);
  
  const driverMeta = getDriverMetadata(driver);

  useEffect(() => {
    if (isOpen && driverMeta.createAction === 'none') {
      toast.info(t('common.mongoCreateDbHint'));
      onClose();
    }
  }, [isOpen, driverMeta.createAction, onClose, t]);

  async function handleCreate() {
    if (!name.trim()) return;

    setLoading(true);
    try {
      let query = '';
      
      if (driverMeta.createAction === 'schema') {
        query = `CREATE SCHEMA "${name}"`;
      } else if (driverMeta.createAction === 'database') {
        query = `CREATE DATABASE \`${name}\``;
      } else {
        toast.error(t('database.creationNotSupported'));
        return;
      }

      const result = await executeQuery(sessionId, query);
      
      if (result.success) {
        const successKey = driverMeta.createAction === 'schema' 
          ? 'database.schemaCreateSuccess' 
          : 'database.databaseCreateSuccess';
        toast.success(t(successKey));
        onCreated();
        onClose();
        setName('');
      } else {
        const errorKey = driverMeta.createAction === 'schema'
          ? 'database.schemaCreateError'
          : 'database.databaseCreateError';
        toast.error(t(errorKey), {
          description: result.error
        });
      }
    } catch (err) {
      toast.error(t('common.error'), {
        description: err instanceof Error ? err.message : t('common.unknownError')
      });
    } finally {
      setLoading(false);
    }
  }

  function handleOpenChange(open: boolean) {
    if (!open) {
      onClose();
      setName('');
    }
  }

  if (driverMeta.createAction === 'none') {
    return null;
  }

  const titleKey = driverMeta.createAction === 'schema' 
    ? 'database.newSchema' 
    : 'database.newDatabase';
  
  const nameLabelKey = driverMeta.createAction === 'schema'
    ? 'database.schemaNameLabel'
    : 'database.databaseNameLabel';

  return (
    <Dialog open={isOpen} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>
            {t(titleKey)}
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-2 py-4">
          <label className="text-sm font-medium">{t(nameLabelKey)}</label>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={driverMeta.createAction === 'schema' ? t('database.schemaNamePlaceholder') : t('database.databaseNamePlaceholder')}
            autoFocus
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleCreate();
            }}
          />
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={loading}>
            {t('common.cancel')}
          </Button>
          <Button onClick={handleCreate} disabled={loading || !name.trim()}>
            {loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            {t('common.create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
