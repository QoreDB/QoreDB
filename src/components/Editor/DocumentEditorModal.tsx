// SPDX-License-Identifier: Apache-2.0

import { Loader2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { DangerConfirmDialog } from '@/components/Guard/DangerConfirmDialog';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { insertRow, type RowData, updateRow } from '../../lib/tauri';
import { MongoEditor } from './MongoEditor';

interface DocumentEditorModalProps {
  isOpen: boolean;
  onClose: () => void;
  mode: 'insert' | 'edit';
  initialData?: string;
  sessionId: string;
  database: string;
  collection: string;
  originalId?: import('../../lib/tauri').Value;
  onSuccess: () => void;
  readOnly?: boolean;
  environment?: 'development' | 'staging' | 'production';
  connectionName?: string;
  connectionDatabase?: string;
}

export function DocumentEditorModal({
  isOpen,
  onClose,
  mode,
  initialData = '{}',
  sessionId,
  database,
  collection,
  originalId,
  onSuccess,
  readOnly = false,
  environment = 'development',
  connectionName,
  connectionDatabase,
}: DocumentEditorModalProps) {
  const { t } = useTranslation();
  const [value, setValue] = useState(initialData);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [pendingAction, setPendingAction] = useState<null | (() => Promise<void>)>(null);
  const confirmationLabel = (connectionDatabase || connectionName || 'PROD').trim() || 'PROD';
  const mutationDescription =
    mode === 'insert'
      ? t('environment.mutationConfirmInsert', { table: collection })
      : t('environment.mutationConfirmUpdate', { table: collection });

  // Reset state when opening
  useEffect(() => {
    if (isOpen) {
      if (mode === 'insert' && (initialData === '{}' || !initialData)) {
        setValue('{\n  \n}');
      } else {
        setValue(initialData);
      }
      setError(null);
    }
  }, [isOpen, initialData, mode]);

  async function handleSave() {
    if (readOnly) {
      toast.error(t('environment.blocked'));
      return;
    }

    setError(null);

    try {
      // 1. Validate JSON
      let parsed: unknown;
      try {
        parsed = JSON.parse(value);
      } catch {
        setError(t('document.invalidJson'));
        setLoading(false);
        return;
      }

      // 2. Prepare RowData
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        setError(t('document.invalidJson'));
        setLoading(false);
        return;
      }
      const rowData: RowData = { columns: {} };
      for (const [k, v] of Object.entries(parsed as Record<string, unknown>)) {
        rowData.columns[k] = v as unknown as import('../../lib/tauri').Value;
      }

      if (environment !== 'development') {
        setPendingAction(() => () => handleSaveConfirmed(rowData, true));
        setConfirmOpen(true);
        return;
      }

      await handleSaveConfirmed(rowData, false);
    } catch (err) {
      console.error(err);
      setError(err instanceof Error ? err.message : t('common.unknownError'));
    }
  }

  async function handleSaveConfirmed(rowData: RowData, acknowledgedDangerous: boolean) {
    setLoading(true);
    setError(null);
    try {
      if (mode === 'insert') {
        const result = await insertRow(
          sessionId,
          database,
          '',
          collection,
          rowData,
          acknowledgedDangerous
        );
        if (result.success) {
          toast.success(t('document.insertSuccess'));
          onSuccess();
          onClose();
        } else {
          setError(result.error || t('rowModal.insertError'));
        }
      } else {
        if (originalId === undefined) {
          setError('Missing original ID for update');
          return;
        }

        const pkData: RowData = { columns: { _id: originalId } };

        const result = await updateRow(
          sessionId,
          database,
          '',
          collection,
          pkData,
          rowData,
          acknowledgedDangerous
        );
        if (result.success) {
          toast.success(t('document.updateSuccess'));
          onSuccess();
          onClose();
        } else {
          setError(result.error || t('rowModal.updateError'));
        }
      }
    } catch (err) {
      console.error(err);
      setError(err instanceof Error ? err.message : t('common.unknownError'));
    } finally {
      setLoading(false);
    }
  }

  return (
    <>
      <Dialog open={isOpen} onOpenChange={open => !open && onClose()}>
        <DialogContent className="max-w-3xl h-[80vh] flex flex-col p-0 gap-0">
          <DialogHeader className="p-4 border-b border-border">
            <DialogTitle>{mode === 'insert' ? t('document.new') : t('document.edit')}</DialogTitle>
          </DialogHeader>

          <div className="flex-1 overflow-hidden min-h-0 relative">
            <MongoEditor value={value} onChange={setValue} readOnly={readOnly || loading} />
          </div>

          {error && (
            <div className="bg-destructive/10 text-destructive text-sm p-2 px-4 border-t border-destructive/20">
              {error}
            </div>
          )}

          <DialogFooter className="p-4 border-t border-border bg-background/50 backdrop-blur-sm z-10">
            <Button variant="outline" onClick={onClose} disabled={loading}>
              {t('common.cancel')}
            </Button>
            <Button onClick={handleSave} disabled={loading || readOnly}>
              {loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {t('common.save')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <DangerConfirmDialog
        open={confirmOpen}
        onOpenChange={open => {
          setConfirmOpen(open);
          if (!open) {
            setPendingAction(null);
          }
        }}
        title={t('environment.mutationConfirmTitle')}
        description={mutationDescription}
        confirmationLabel={environment === 'production' ? confirmationLabel : undefined}
        confirmLabel={t('common.confirm')}
        loading={loading}
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
