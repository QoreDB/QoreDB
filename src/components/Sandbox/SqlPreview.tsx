// SPDX-License-Identifier: Apache-2.0

import { MySQL, PostgreSQL, sql } from '@codemirror/lang-sql';
import { EditorState } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import { EditorView, lineNumbers } from '@codemirror/view';
import { useEffect, useRef } from 'react';
import { useTheme } from '@/hooks/useTheme';
import { Driver } from '@/lib/drivers';
import { cn } from '@/lib/utils';

interface SqlPreviewProps {
  value: string;
  dialect?: Driver;
  className?: string;
}

export function SqlPreview({ value, dialect = Driver.Postgres, className }: SqlPreviewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const { isDark } = useTheme();

  useEffect(() => {
    if (!containerRef.current) return;

    const sqlDialect = dialect === Driver.Mysql ? MySQL : PostgreSQL;
    const extensions = [
      lineNumbers(),
      sql({ dialect: sqlDialect }),
      EditorView.editable.of(false),
      EditorView.theme({
        '&': { height: '100%' },
        '.cm-scroller': { overflow: 'auto' },
        '.cm-content': { fontFamily: 'var(--font-mono)' },
      }),
    ];

    if (isDark) {
      extensions.push(oneDark);
    }

    const state = EditorState.create({
      doc: value,
      extensions,
    });

    const view = new EditorView({
      state,
      parent: containerRef.current,
    });

    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [dialect, isDark, value]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({
        changes: {
          from: 0,
          to: view.state.doc.length,
          insert: value,
        },
      });
    }
  }, [value]);

  return <div ref={containerRef} className={cn('h-full', className)} />;
}
