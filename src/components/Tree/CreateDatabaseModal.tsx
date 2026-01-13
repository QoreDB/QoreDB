import { useState } from 'react';
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

  async function handleCreate() {
    if (!name.trim()) return;

    setLoading(true);
    try {
      let query = '';
      if (driver === 'postgres' || driver === 'mysql' || driver === 'mariadb') {
        query = `CREATE DATABASE "${name}"`;
        if (driver === 'mysql' || driver === 'mariadb') {
           query = `CREATE DATABASE \`${name}\``;
        }
      } else if (driver === 'mongodb') {
        toast.info(t('common.mongoCreateDbHint') || 'For MongoDB, simply insert data into a namespace to create it.');
        onClose();
        return;
      } else {
        toast.error('Driver not supported for database creation');
        return;
      }

      const result = await executeQuery(sessionId, query);
      
      if (result.success) {
        toast.success(t('database.createSuccess') || 'Database created successfully');
        onCreated();
        onClose();
        setName('');
      } else {
        toast.error(t('database.createError') || 'Failed to create database', {
          description: result.error
        });
      }
    } catch (err) {
      toast.error(t('common.error'), {
        description: err instanceof Error ? err.message : 'Unknown error'
      });
    } finally {
      setLoading(false);
    }
  }

  function handleOpenChange(open: boolean) {
    if (!open) {
      onClose();
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>
            {t('database.newTitle') || 'New Database'}
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-2 py-4">
          <label className="text-sm font-medium">Database Name</label>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="my_new_db"
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
            {t('common.create') || 'Create'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
