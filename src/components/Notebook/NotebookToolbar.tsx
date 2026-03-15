// SPDX-License-Identifier: Apache-2.0

import {
  Code,
  Download,
  Eraser,
  FileText,
  PlayCircle,
  Plus,
  Save,
  Square,
  Upload,
} from 'lucide-react';
import { useCallback, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Tooltip } from '@/components/ui/tooltip';
import type { CellType } from '@/lib/notebookTypes';

interface NotebookToolbarProps {
  title: string;
  isDirty: boolean;
  isExecuting: boolean;
  onTitleChange: (title: string) => void;
  onSave: () => void;
  onAddCell: (type: CellType) => void;
  onExecuteAll: () => void;
  onClearAll: () => void;
  onCancel: () => void;
  onImport: () => void;
  onExport: (format: 'markdown' | 'html') => void;
}

export function NotebookToolbar({
  title,
  isDirty,
  isExecuting,
  onTitleChange,
  onSave,
  onAddCell,
  onExecuteAll,
  onClearAll,
  onCancel,
  onImport,
  onExport,
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

      {/* Execution actions */}
      <div className="flex items-center gap-1">
        {isExecuting ? (
          <Tooltip content={t('notebook.cancelExecution')}>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 gap-1 text-destructive"
              onClick={onCancel}
            >
              <Square size={14} />
              <span className="text-xs">{t('notebook.cancelExecution')}</span>
            </Button>
          </Tooltip>
        ) : (
          <Tooltip content={`${t('notebook.executeAll')} (Ctrl+Shift+A)`}>
            <Button variant="ghost" size="sm" className="h-7 gap-1" onClick={onExecuteAll}>
              <PlayCircle size={14} />
              <span className="text-xs">{t('notebook.executeAll')}</span>
            </Button>
          </Tooltip>
        )}

        <Tooltip content={t('notebook.clearAll')}>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={onClearAll}
            disabled={isExecuting}
          >
            <Eraser size={14} />
          </Button>
        </Tooltip>
      </div>

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

      {/* Import */}
      <Tooltip content={t('notebook.import')}>
        <Button variant="ghost" size="icon" className="h-7 w-7" onClick={onImport}>
          <Upload size={14} />
        </Button>
      </Tooltip>

      {/* Export */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon" className="h-7 w-7">
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

      {/* Save */}
      <Tooltip content={t('notebook.save')}>
        <Button variant="ghost" size="icon" className="h-7 w-7" onClick={onSave}>
          <Save size={14} />
        </Button>
      </Tooltip>
    </div>
  );
}
