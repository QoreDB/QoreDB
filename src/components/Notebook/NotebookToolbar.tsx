// SPDX-License-Identifier: Apache-2.0

import {
  Code,
  Download,
  Eraser,
  FileText,
  PlayCircle,
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
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import type { CellType } from '@/lib/notebookTypes';

interface NotebookToolbarProps {
  title: string;
  isDirty: boolean;
  isExecuting: boolean;
  canUndo: boolean;
  canRedo: boolean;
  hasVariables: boolean;
  onTitleChange: (title: string) => void;
  onSave: () => void;
  onSaveAs: () => void;
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
  onTitleChange,
  onSave,
  onSaveAs,
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
    <div className="flex items-center gap-2 px-3 py-2 border-b border-border bg-background shrink-0">
      {/* Title */}
      <div className="flex items-center gap-1 flex-1 min-w-0">
        {isDirty && (
          <span className="text-muted-foreground text-sm" title={t('notebook.unsavedChanges')}>
            ●
          </span>
        )}
        {editingTitle ? (
          <input
            ref={inputRef}
            value={title}
            onChange={e => onTitleChange(e.target.value)}
            onBlur={handleTitleBlur}
            onKeyDown={handleTitleKeyDown}
            className="text-sm font-medium bg-transparent border-b border-border focus:border-accent focus:outline-none px-1 py-0.5 min-w-30"
          />
        ) : (
          <button
            type="button"
            onClick={handleTitleClick}
            className="text-sm font-medium truncate hover:text-accent transition-colors text-left"
          >
            {title}
          </button>
        )}
      </div>

      {/* Undo / Redo */}
      <div className="flex items-center gap-0.5">
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          onClick={onUndo}
          disabled={!canUndo}
          title={`${t('notebook.undo')} (Ctrl+Z)`}
        >
          <Undo2 size={14} />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          onClick={onRedo}
          disabled={!canRedo}
          title={`${t('notebook.redo')} (Ctrl+Shift+Z)`}
        >
          <Redo2 size={14} />
        </Button>
      </div>

      {/* Separator */}
      <div className="w-px h-4 bg-border" />

      {/* Execution actions */}
      <div className="flex items-center gap-1">
        {isExecuting ? (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 gap-1 text-destructive"
            onClick={onCancel}
            title={t('notebook.cancelExecution')}
          >
            <Square size={14} />
            <span className="text-xs">{t('notebook.cancelExecution')}</span>
          </Button>
        ) : (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 gap-1"
            onClick={onExecuteAll}
            title={`${t('notebook.executeAll')} (Ctrl+Shift+A)`}
          >
            <PlayCircle size={14} />
            <span className="text-xs">{t('notebook.executeAll')}</span>
          </Button>
        )}

        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          onClick={onClearAll}
          disabled={isExecuting}
          title={t('notebook.clearAll')}
        >
          <Eraser size={14} />
        </Button>
      </div>

      {/* Separator */}
      <div className="w-px h-4 bg-border" />

      {/* Add cell */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="sm" className="h-7 gap-1">
            <Plus size={14} />
            <span className="text-xs">{t('notebook.addCellBelow')}</span>
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

      {/* Variables */}
      <Button
        variant={hasVariables ? 'secondary' : 'ghost'}
        size="icon"
        className="h-7 w-7"
        onClick={onToggleVariables}
        title={t('notebook.toggleVariables')}
      >
        <Variable size={14} />
      </Button>

      {/* Separator */}
      <div className="w-px h-4 bg-border" />

      {/* Import */}
      <Button
        variant="ghost"
        size="icon"
        className="h-7 w-7"
        onClick={onImport}
        title={t('notebook.import')}
      >
        <Upload size={14} />
      </Button>

      {/* Export */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon" className="h-7 w-7" title={t('notebook.export')}>
            <Download size={14} />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onClick={() => onExport('markdown')}>
            {t('notebook.exportMarkdown')}
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => onExport('html')}>
            {t('notebook.exportHtml')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Save / Save As */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            title={`${t('notebook.save')} (Ctrl+S)`}
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
    </div>
  );
}
