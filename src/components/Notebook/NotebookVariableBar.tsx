// SPDX-License-Identifier: Apache-2.0

import { Plus, X } from 'lucide-react';
import { useCallback, useState } from 'react';
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tooltip } from '@/components/ui/tooltip';
import type { NotebookVariable } from '@/lib/notebookTypes';

interface NotebookVariableBarProps {
  variables: Record<string, NotebookVariable>;
  onUpdateVariable: (name: string, value: string) => void;
  onAddVariable: (variable: NotebookVariable) => void;
  onRemoveVariable: (name: string) => void;
}

const VARIABLE_TYPES = ['text', 'number', 'date', 'select'] as const;

export function NotebookVariableBar({
  variables,
  onUpdateVariable,
  onAddVariable,
  onRemoveVariable,
}: NotebookVariableBarProps) {
  const { t } = useTranslation();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [newVar, setNewVar] = useState<Partial<NotebookVariable>>({ type: 'text' });

  const entries = Object.values(variables);

  const handleAddVariable = useCallback(() => {
    if (!newVar.name?.trim()) return;
    onAddVariable({
      name: newVar.name.trim(),
      type: newVar.type ?? 'text',
      defaultValue: newVar.defaultValue,
      description: newVar.description,
      options: newVar.options,
    });
    setNewVar({ type: 'text' });
    setDialogOpen(false);
  }, [newVar, onAddVariable]);

  return (
    <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border bg-muted/30 overflow-x-auto shrink-0">
      {entries.map(v => (
        <div key={v.name} className="flex items-center gap-1 shrink-0">
          <span className="text-xs font-mono text-muted-foreground">${v.name}</span>
          {v.type === 'select' && v.options ? (
            <Select
              value={v.currentValue ?? v.defaultValue ?? ''}
              onValueChange={val => onUpdateVariable(v.name, val)}
            >
              <SelectTrigger className="h-6 text-xs w-auto min-w-20">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {v.options.map(opt => (
                  <SelectItem key={opt} value={opt}>
                    {opt}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : (
            <Input
              type={v.type === 'number' ? 'number' : v.type === 'date' ? 'date' : 'text'}
              value={v.currentValue ?? v.defaultValue ?? ''}
              onChange={e => onUpdateVariable(v.name, e.target.value)}
              placeholder={v.description ?? v.name}
              className="h-6 text-xs w-24"
            />
          )}
          <Tooltip content={t('notebook.removeVariable')}>
            <Button
              variant="ghost"
              size="icon"
              className="h-5 w-5 text-muted-foreground hover:text-destructive"
              onClick={() => onRemoveVariable(v.name)}
            >
              <X size={10} />
            </Button>
          </Tooltip>
        </div>
      ))}

      <Tooltip content={t('notebook.addVariable')}>
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6 shrink-0"
          onClick={() => setDialogOpen(true)}
        >
          <Plus size={12} />
        </Button>
      </Tooltip>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('notebook.addVariable')}</DialogTitle>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <Input
              placeholder={t('notebook.variableName')}
              value={newVar.name ?? ''}
              onChange={e => setNewVar(p => ({ ...p, name: e.target.value }))}
            />
            <Select
              value={newVar.type ?? 'text'}
              onValueChange={val =>
                setNewVar(p => ({ ...p, type: val as NotebookVariable['type'] }))
              }
            >
              <SelectTrigger>
                <SelectValue placeholder={t('notebook.variableType')} />
              </SelectTrigger>
              <SelectContent>
                {VARIABLE_TYPES.map(vt => (
                  <SelectItem key={vt} value={vt}>
                    {vt}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Input
              placeholder={t('notebook.variableDefault')}
              value={newVar.defaultValue ?? ''}
              onChange={e => setNewVar(p => ({ ...p, defaultValue: e.target.value }))}
            />
            <Input
              placeholder={t('notebook.variableDescription')}
              value={newVar.description ?? ''}
              onChange={e => setNewVar(p => ({ ...p, description: e.target.value }))}
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDialogOpen(false)}>
              {t('notebook.unsavedCancel')}
            </Button>
            <Button onClick={handleAddVariable} disabled={!newVar.name?.trim()}>
              {t('notebook.addVariable')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
