// SPDX-License-Identifier: Apache-2.0

import { Code, FileText, Plus, Save } from 'lucide-react';
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
  onTitleChange: (title: string) => void;
  onSave: () => void;
  onAddCell: (type: CellType) => void;
}

export function NotebookToolbar({
  title,
  isDirty,
  onTitleChange,
  onSave,
  onAddCell,
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
            className="text-sm font-medium bg-transparent border-b border-border focus:border-accent focus:outline-none px-1 py-0.5 min-w-[120px]"
            autoFocus
          />
        ) : (
          <button
            onClick={handleTitleClick}
            className="text-sm font-medium truncate hover:text-accent transition-colors text-left"
          >
            {title}
          </button>
        )}
      </div>

      {/* Actions */}
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

      <Tooltip content={t('notebook.save')}>
        <Button variant="ghost" size="icon" className="h-7 w-7" onClick={onSave}>
          <Save size={14} />
        </Button>
      </Tooltip>
    </div>
  );
}
