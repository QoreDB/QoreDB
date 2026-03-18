// SPDX-License-Identifier: Apache-2.0

import { Eye, Pencil } from 'lucide-react';
import { useCallback, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Button } from '@/components/ui/button';
import { Tooltip } from '@/components/ui/tooltip';
import type { NotebookCell } from '@/lib/notebookTypes';

interface MarkdownCellProps {
  cell: NotebookCell;
  onSourceChange: (source: string) => void;
}

export function MarkdownCell({ cell, onSourceChange }: MarkdownCellProps) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(!cell.source.trim());
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Escape' && cell.source.trim()) {
        setEditing(false);
      }
    },
    [cell.source]
  );

  const switchToEdit = useCallback(() => {
    setEditing(true);
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, []);

  if (editing) {
    return (
      <div className="relative">
        <textarea
          ref={textareaRef}
          value={cell.source}
          onChange={e => onSourceChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={t('notebook.markdownPlaceholder')}
          className="w-full min-h-24 p-3 bg-muted/20 rounded-md text-sm font-mono resize-y focus:outline-none focus:ring-1 focus:ring-ring"
          // biome-ignore lint/a11y/noAutofocus: intentional focus on mode switch
          autoFocus
        />
        {cell.source.trim() && (
          <div className="absolute top-1.5 right-1.5">
            <Tooltip content={t('notebook.previewMarkdown')} side="left">
              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6"
                onClick={() => setEditing(false)}
              >
                <Eye size={12} />
              </Button>
            </Tooltip>
          </div>
        )}
        <span className="text-[10px] text-muted-foreground/60 px-1 mt-0.5 block">
          Esc {t('notebook.previewMarkdown')}
        </span>
      </div>
    );
  }

  if (!cell.source.trim()) {
    return (
      <button
        type="button"
        onClick={switchToEdit}
        className="w-full text-left p-3 text-sm text-muted-foreground italic cursor-text rounded-md bg-muted/10 hover:bg-muted/20 transition-colors"
      >
        {t('notebook.markdownPlaceholder')}
      </button>
    );
  }

  return (
    <div className="group/md relative">
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: double-click to edit */}
      <div
        onDoubleClick={switchToEdit}
        className="prose prose-sm dark:prose-invert max-w-none p-3 cursor-text rounded-md hover:bg-muted/20 transition-colors"
      >
        <Markdown remarkPlugins={[remarkGfm]}>{cell.source}</Markdown>
      </div>
      <div className="absolute top-1.5 right-1.5 opacity-0 group-hover/md:opacity-100 transition-opacity">
        <Tooltip content={t('notebook.editMarkdown')} side="left">
          <Button variant="ghost" size="icon" className="h-6 w-6 bg-card/80" onClick={switchToEdit}>
            <Pencil size={12} />
          </Button>
        </Tooltip>
      </div>
    </div>
  );
}
