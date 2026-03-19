// SPDX-License-Identifier: Apache-2.0

import { Plus, Variable, X } from 'lucide-react';
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
    <div className="flex items-center gap-3 px-4 py-2 border-b border-border bg-muted/10 overflow-x-auto shrink-0">
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground shrink-0">
        <Variable size={13} />
        <span className="font-medium uppercase tracking-wider">{t('notebook.variables')}</span>
      </div>

      <div className="h-4 w-px bg-border/50" />

      {entries.map(v => (
        <div
          key={v.name}
          className="flex items-center gap-1.5 shrink-0 bg-background/60 rounded-md border border-border/50 px-2 py-1"
        >
          <label className="text-xs font-mono text-muted-foreground shrink-0">{v.name}</label>
          <span className="text-muted-foreground/40">=</span>
          {v.type === 'select' && v.options ? (
            <Select
              value={v.currentValue ?? v.defaultValue ?? ''}
              onValueChange={val => onUpdateVariable(v.name, val)}
            >
              <SelectTrigger className="h-6 text-xs w-auto min-w-[80px] border-0 bg-transparent px-1">
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
              placeholder={v.description ?? v.defaultValue ?? ''}
              className="h-6 text-xs w-24 border-0 bg-transparent px-1 focus-visible:ring-0 focus-visible:ring-offset-0 focus-visible:border-b focus-visible:border-accent focus-visible:rounded-none"
            />
          )}
          <Tooltip content={t('notebook.removeVariable')}>
            <Button
              variant="ghost"
              size="icon"
              className="h-5 w-5 text-muted-foreground/50 hover:text-destructive"
              onClick={() => onRemoveVariable(v.name)}
            >
              <X size={10} />
            </Button>
          </Tooltip>
        </div>
      ))}

      <Tooltip content={t('notebook.addVariable')}>
        <Button
          variant="outline"
          size="icon"
          className="h-6 w-6 shrink-0 rounded-full"
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
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                {t('notebook.variableName')}
              </label>
              <Input
                placeholder="my_variable"
                value={newVar.name ?? ''}
                onChange={e => setNewVar(p => ({ ...p, name: e.target.value }))}
                className="font-mono"
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                {t('notebook.variableType')}
              </label>
              <Select
                value={newVar.type ?? 'text'}
                onValueChange={val =>
                  setNewVar(p => ({ ...p, type: val as NotebookVariable['type'] }))
                }
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {VARIABLE_TYPES.map(vt => (
                    <SelectItem key={vt} value={vt}>
                      {vt}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                {t('notebook.variableDefault')}
              </label>
              <Input
                placeholder="value"
                value={newVar.defaultValue ?? ''}
                onChange={e => setNewVar(p => ({ ...p, defaultValue: e.target.value }))}
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                {t('notebook.variableDescription')}
              </label>
              <Input
                placeholder={t('notebook.variableDescriptionHint')}
                value={newVar.description ?? ''}
                onChange={e => setNewVar(p => ({ ...p, description: e.target.value }))}
              />
            </div>
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
