// SPDX-License-Identifier: Apache-2.0

import { useCallback, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import type { NotebookCell } from '@/lib/notebookTypes';

interface MarkdownCellProps {
  cell: NotebookCell;
  onSourceChange: (source: string) => void;
}

export function MarkdownCell({ cell, onSourceChange }: MarkdownCellProps) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(!cell.source.trim());
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleDoubleClick = useCallback(() => {
    setEditing(true);
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, []);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      setEditing(false);
    }
  }, []);

  const handleBlur = useCallback(() => {
    if (cell.source.trim()) {
      setEditing(false);
    }
  }, [cell.source]);

  if (editing) {
    return (
      <textarea
        ref={textareaRef}
        value={cell.source}
        onChange={e => onSourceChange(e.target.value)}
        onKeyDown={handleKeyDown}
        onBlur={handleBlur}
        placeholder={t('notebook.markdownPlaceholder')}
        className="w-full min-h-[76px] p-3 bg-transparent border border-border rounded-md text-sm font-mono resize-y focus:outline-none focus:ring-1 focus:ring-ring"
        autoFocus
      />
    );
  }

  if (!cell.source.trim()) {
    return (
      <div
        onClick={() => setEditing(true)}
        className="p-3 text-sm text-muted-foreground italic cursor-text border border-dashed border-border rounded-md hover:border-muted-foreground/50"
      >
        {t('notebook.markdownPlaceholder')}
      </div>
    );
  }

  return (
    <div
      onDoubleClick={handleDoubleClick}
      className="prose prose-sm dark:prose-invert max-w-none p-3 cursor-text rounded-md hover:bg-muted/30 transition-colors"
    >
      <Markdown remarkPlugins={[remarkGfm]}>{cell.source}</Markdown>
    </div>
  );
}
