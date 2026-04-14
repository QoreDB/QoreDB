// SPDX-License-Identifier: Apache-2.0

import {
  Code,
  Database,
  Download,
  Eraser,
  FileText,
  FolderOpen,
  Loader2,
  MoreHorizontal,
  Play,
  Plus,
  Redo2,
  Save,
  Square,
  Undo2,
  Upload,
  Variable,
} from 'lucide-react';
import { useCallback, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tooltip } from '@/components/ui/tooltip';
import type { CellType } from '@/lib/notebookTypes';
import type { Namespace } from '@/lib/tauri';
import { getShortcut } from '@/utils/platform';

function formatNamespace(ns: Namespace): string {
  return ns.schema ? `${ns.database}.${ns.schema}` : ns.database;
}

function getNamespaceKey(ns: Namespace): string {
  return `${ns.database}:${ns.schema ?? ''}`;
}

interface NotebookToolbarProps {
  title: string;
  isDirty: boolean;
  isExecuting: boolean;
  canUndo: boolean;
  canRedo: boolean;
  hasVariables: boolean;
  cellCount: number;
  namespaces: Namespace[];
  selectedNamespace: Namespace | null;
  onNamespaceChange: (ns: Namespace | null) => void;
  onTitleChange: (title: string) => void;
  onSave: () => void;
  onSaveAs: () => void;
  onOpen: () => void;
  onAddCell: (type: CellType) => void;
  onExecuteAll: () => void;
  onClearAll: () => void;
  onCancel: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onImport: () => void;
  onExport: (format: 'markdown' | 'html') => void;
  onToggleVariables: () => void;
}

export function NotebookToolbar({
  title,
  isDirty,
  isExecuting,
  canUndo,
  canRedo,
  hasVariables,
  cellCount,
  namespaces,
  selectedNamespace,
  onNamespaceChange,
  onTitleChange,
  onSave,
  onSaveAs,
  onOpen,
  onAddCell,
  onExecuteAll,
  onClearAll,
  onCancel,
  onUndo,
  onRedo,
  onImport,
  onExport,
  onToggleVariables,
}: NotebookToolbarProps) {
  const { t } = useTranslation();
  const [editingTitle, setEditingTitle] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleTitleClick = useCallback(() => {
    setEditingTitle(true);
    requestAnimationFrame(() => inputRef.current?.select());
  }, []);

  const handleTitleBlur = useCallback(() => {
    setEditingTitle(false);
  }, []);

  const handleTitleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === 'Escape') {
      setEditingTitle(false);
    }
  }, []);

  return (
    <div className="flex items-center gap-2 p-2 border-b border-border bg-muted/20 shrink-0">
      {/* --- Primary: Run All / Cancel --- */}
      {isExecuting ? (
        <Tooltip content={t('notebook.cancelExecution')}>
          <Button
            variant="destructive"
            size="sm"
            className="h-7 gap-1.5 text-xs"
            onClick={onCancel}
          >
            <Square size={13} />
            <span>{t('notebook.cancelExecution')}</span>
          </Button>
        </Tooltip>
      ) : (
        <Tooltip
          content={`${t('notebook.executeAll')} (${getShortcut('A', { symbol: true, shift: true })})`}
        >
          <Button
            data-tour="notebook-run-all"
            variant="default"
            size="sm"
            className="h-7 gap-1.5 text-xs"
            onClick={onExecuteAll}
          >
            {isExecuting ? <Loader2 size={13} className="animate-spin" /> : <Play size={13} />}
            <span>{t('notebook.executeAll')}</span>
          </Button>
        </Tooltip>
      )}

      {/* --- Namespace selector --- */}
      {namespaces.length > 0 && (
        <Select
          value={selectedNamespace ? getNamespaceKey(selectedNamespace) : undefined}
          onValueChange={value => {
            const selected = namespaces.find(ns => getNamespaceKey(ns) === value);
            onNamespaceChange(selected ?? null);
          }}
        >
          <SelectTrigger size="sm" className="h-7 max-w-50 text-xs gap-1 border-border/50">
            <Database size={12} className="shrink-0 text-muted-foreground" />
            <SelectValue placeholder={t('notebook.selectNamespace')} />
          </SelectTrigger>
          <SelectContent>
            {namespaces.map(ns => (
              <SelectItem key={getNamespaceKey(ns)} value={getNamespaceKey(ns)}>
                {formatNamespace(ns)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}

      <div className="h-5 w-px bg-border/50" />

      <div className="flex items-center gap-1.5 flex-1 min-w-0">
        {editingTitle ? (
          <input
            ref={inputRef}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            value={title}
            onChange={e => onTitleChange(e.target.value)}
            onBlur={handleTitleBlur}
            onKeyDown={handleTitleKeyDown}
            className="text-sm font-medium bg-transparent border-b border-accent focus:outline-none px-1 py-0.5 min-w-30 max-w-75"
          />
        ) : (
          <button
            type="button"
            onClick={handleTitleClick}
            className="text-sm font-medium truncate hover:text-accent transition-colors text-left max-w-75"
            title={t('notebook.editTitle')}
          >
            {title}
          </button>
        )}
        {isDirty && (
          <span
            className="w-1.5 h-1.5 rounded-full bg-amber-500 shrink-0"
            title={t('notebook.unsavedChanges')}
          />
        )}
        <span className="text-[11px] text-muted-foreground shrink-0 hidden sm:inline-block">
          {t('notebook.cellCountLabel', { count: cellCount })}
        </span>
      </div>

      {/* Undo / Redo */}
      <Tooltip content={`${t('notebook.undo')} (${getShortcut('Z', { symbol: true })})`}>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          onClick={onUndo}
          disabled={!canUndo}
        >
          <Undo2 size={14} />
        </Button>
      </Tooltip>
      <Tooltip
        content={`${t('notebook.redo')} (${getShortcut('Z', { symbol: true, shift: true })})`}
      >
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          onClick={onRedo}
          disabled={!canRedo}
        >
          <Redo2 size={14} />
        </Button>
      </Tooltip>

      <div className="h-5 w-px bg-border/50" />

      {/* Add Cell */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            data-tour="notebook-add-cell"
            variant="ghost"
            size="sm"
            className="h-7 gap-1"
            title={t('notebook.addCellBelow')}
          >
            <Plus size={14} />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onClick={() => onAddCell('sql')}>
            <Code size={14} className="mr-2" />
            {t('notebook.addCellSql')}
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => onAddCell('markdown')}>
            <FileText size={14} className="mr-2" />
            {t('notebook.addCellMarkdown')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <div className="h-5 w-px bg-border/50" />

      {/* Save */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            data-tour="notebook-save"
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            title={`${t('notebook.save')} (${getShortcut('S', { symbol: true })})`}
          >
            <Save size={14} />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onClick={onSave}>
            <Save size={14} className="mr-2" />
            {t('notebook.save')}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={onSaveAs}>{t('notebook.saveAs')}</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Open */}
      <Tooltip content={t('notebook.open')}>
        <Button variant="ghost" size="icon" className="h-7 w-7" onClick={onOpen}>
          <FolderOpen size={14} />
        </Button>
      </Tooltip>

      {/* More actions */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-muted-foreground hover:text-foreground"
            aria-label={t('toolbar.moreActions')}
          >
            <MoreHorizontal size={14} />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-48">
          <DropdownMenuItem onClick={onClearAll} disabled={isExecuting}>
            <Eraser size={14} className="mr-2" />
            {t('notebook.clearAll')}
          </DropdownMenuItem>
          <DropdownMenuCheckboxItem
            checked={hasVariables}
            onCheckedChange={() => onToggleVariables()}
          >
            <Variable size={14} className="mr-2" />
            {t('notebook.toggleVariables')}
          </DropdownMenuCheckboxItem>

          <DropdownMenuSeparator />

          <DropdownMenuItem onClick={onImport}>
            <Upload size={14} className="mr-2" />
            {t('notebook.import')}
          </DropdownMenuItem>
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <Download size={14} className="mr-2" />
              {t('notebook.export')}
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuItem onClick={() => onExport('markdown')}>
                {t('notebook.exportMarkdown')}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => onExport('html')}>
                {t('notebook.exportHtml')}
              </DropdownMenuItem>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
