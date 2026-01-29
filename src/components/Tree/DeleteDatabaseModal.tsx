import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { AlertTriangle } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { dropDatabase, Namespace } from '@/lib/tauri';

interface DeleteDatabaseModalProps {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string;
  namespace: Namespace;
  driver: string;
  environment: string;
  onDeleted: () => void;
}

export function DeleteDatabaseModal({
  isOpen,
  onClose,
  sessionId,
  namespace,
  driver,
  environment,
  onDeleted,
}: DeleteDatabaseModalProps) {
  const { t } = useTranslation();
  const [step, setStep] = useState<1 | 2>(1);
  const [confirmText, setConfirmText] = useState('');
  const [loading, setLoading] = useState(false);

  // Determine what we're deleting based on driver
  const isSchema = driver === 'postgres';
  const displayName = namespace.schema
    ? `${namespace.database}.${namespace.schema}`
    : namespace.database;
  const targetName = namespace.schema || namespace.database;

  const isProduction = environment === 'production';
  const canProceed = confirmText === targetName;

  const handleClose = () => {
    setStep(1);
    setConfirmText('');
    setLoading(false);
    onClose();
  };

  const handleFirstConfirm = () => {
    setStep(2);
  };

  const handleDelete = async () => {
    if (!canProceed) return;

    setLoading(true);
    try {
      const result = await dropDatabase(sessionId, targetName, true);
      if (result.success) {
        toast.success(
          t(isSchema ? 'dropDatabase.schemaSuccess' : 'dropDatabase.success', {
            name: displayName,
          })
        );
        onDeleted();
        handleClose();
      } else {
        toast.error(result.error || t('dropDatabase.failed'));
      }
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      toast.error(message);
    } finally {
      setLoading(false);
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && handleClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-destructive">
            <AlertTriangle size={20} />
            {t(isSchema ? 'dropDatabase.schemaTitle' : 'dropDatabase.title')}
          </DialogTitle>
          <DialogDescription className="pt-2">
            {step === 1 ? (
              <>
                {t(isSchema ? 'dropDatabase.schemaConfirm' : 'dropDatabase.confirm', {
                  name: displayName,
                })}
                {isProduction && (
                  <span className="block mt-2 text-destructive font-semibold">
                    ⚠️ {t('dropDatabase.productionWarning')}
                  </span>
                )}
              </>
            ) : (
              <>
                {t('dropDatabase.typeToConfirm', { name: targetName })}
              </>
            )}
          </DialogDescription>
        </DialogHeader>

        {step === 2 && (
          <div className="py-4">
            <Input
              value={confirmText}
              onChange={(e) => setConfirmText(e.target.value)}
              placeholder={targetName}
              className="font-mono"
              autoFocus
            />
          </div>
        )}

        <DialogFooter className="gap-2 sm:gap-0">
          <Button variant="outline" onClick={handleClose} disabled={loading}>
            {t('common.cancel')}
          </Button>
          {step === 1 ? (
            <Button variant="destructive" onClick={handleFirstConfirm}>
              {t('dropDatabase.continue')}
            </Button>
          ) : (
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={!canProceed || loading}
            >
              {loading ? t('common.deleting') : t('common.delete')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
