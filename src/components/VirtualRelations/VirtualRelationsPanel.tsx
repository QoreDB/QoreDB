import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Link2, Pencil, Trash2, Plus } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';

import {
  Namespace,
  VirtualRelation,
  listVirtualRelations,
  deleteVirtualRelation,
} from '@/lib/tauri';
import { VirtualRelationDialog } from './VirtualRelationDialog';

interface VirtualRelationsPanelProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessionId: string;
  connectionId: string;
  namespace: Namespace;
  onChanged: () => void;
}

export function VirtualRelationsPanel({
  open,
  onOpenChange,
  sessionId,
  connectionId,
  namespace,
  onChanged,
}: VirtualRelationsPanelProps) {
  const { t } = useTranslation();
  const [relations, setRelations] = useState<VirtualRelation[]>([]);
  const [loading, setLoading] = useState(false);
  const [editDialogOpen, setEditDialogOpen] = useState(false);
  const [editingRelation, setEditingRelation] = useState<VirtualRelation | undefined>();
  const [addDialogOpen, setAddDialogOpen] = useState(false);

  const loadRelations = useCallback(async () => {
    if (!connectionId) return;
    setLoading(true);
    try {
      const result = await listVirtualRelations(connectionId);
      if (result.success && result.relations) {
        // Filter to current namespace
        const filtered = result.relations.filter(
          r =>
            r.source_database === namespace.database &&
            (r.source_schema ?? null) === (namespace.schema ?? null)
        );
        setRelations(filtered);
      }
    } finally {
      setLoading(false);
    }
  }, [connectionId, namespace]);

  useEffect(() => {
    if (open) {
      loadRelations();
    }
  }, [open, loadRelations]);

  async function handleDelete(relation: VirtualRelation) {
    const result = await deleteVirtualRelation(connectionId, relation.id);
    if (result.success) {
      toast.success(t('virtualRelations.deleteSuccess'));
      loadRelations();
      onChanged();
    } else {
      toast.error(result.error ?? t('common.error'));
    }
  }

  function handleEdit(relation: VirtualRelation) {
    setEditingRelation(relation);
    setEditDialogOpen(true);
  }

  function handleEditSaved() {
    loadRelations();
    onChanged();
  }

  function handleAddSaved() {
    loadRelations();
    onChanged();
  }

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Link2 size={16} />
              {t('virtualRelations.title')}
            </DialogTitle>
          </DialogHeader>

          <p className="text-xs text-muted-foreground mb-2">{t('virtualRelations.description')}</p>

          <div className="flex justify-end mb-2">
            <Button size="sm" variant="outline" onClick={() => setAddDialogOpen(true)}>
              <Plus size={14} className="mr-1" />
              {t('virtualRelations.addFromTable')}
            </Button>
          </div>

          {loading ? (
            <div className="text-sm text-muted-foreground py-4 text-center">...</div>
          ) : relations.length === 0 ? (
            <div className="text-sm text-muted-foreground py-8 text-center">
              {t('virtualRelations.noRelations')}
            </div>
          ) : (
            <div className="border rounded-md overflow-hidden">
              <table className="w-full text-xs">
                <thead>
                  <tr className="border-b bg-muted/30">
                    <th className="text-left px-3 py-2 font-medium">
                      {t('virtualRelations.sourceTable')}
                    </th>
                    <th className="text-left px-3 py-2 font-medium">
                      {t('virtualRelations.sourceColumn')}
                    </th>
                    <th className="text-left px-3 py-2 font-medium">
                      {t('virtualRelations.referencedTable')}
                    </th>
                    <th className="text-left px-3 py-2 font-medium">
                      {t('virtualRelations.referencedColumn')}
                    </th>
                    <th className="text-left px-3 py-2 font-medium">
                      {t('virtualRelations.label')}
                    </th>
                    <th className="w-20" />
                  </tr>
                </thead>
                <tbody>
                  {relations.map(rel => (
                    <tr key={rel.id} className="border-b last:border-b-0 hover:bg-muted/20">
                      <td className="px-3 py-1.5 font-mono">{rel.source_table}</td>
                      <td className="px-3 py-1.5 font-mono">{rel.source_column}</td>
                      <td className="px-3 py-1.5 font-mono">{rel.referenced_table}</td>
                      <td className="px-3 py-1.5 font-mono">{rel.referenced_column}</td>
                      <td className="px-3 py-1.5 text-muted-foreground">{rel.label ?? '-'}</td>
                      <td className="px-2 py-1.5">
                        <div className="flex gap-1 justify-end">
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-6 w-6"
                            onClick={() => handleEdit(rel)}
                          >
                            <Pencil size={12} />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-6 w-6 text-destructive hover:text-destructive"
                            onClick={() => handleDelete(rel)}
                          >
                            <Trash2 size={12} />
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </DialogContent>
      </Dialog>

      {/* Edit dialog */}
      <VirtualRelationDialog
        open={editDialogOpen}
        onOpenChange={setEditDialogOpen}
        sessionId={sessionId}
        connectionId={connectionId}
        namespace={namespace}
        existingRelation={editingRelation}
        onSaved={handleEditSaved}
      />

      {/* Add dialog */}
      <VirtualRelationDialog
        open={addDialogOpen}
        onOpenChange={setAddDialogOpen}
        sessionId={sessionId}
        connectionId={connectionId}
        namespace={namespace}
        onSaved={handleAddSaved}
      />
    </>
  );
}
