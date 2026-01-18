import { useEffect, useRef, useMemo } from 'react';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view';
import { sql, PostgreSQL, MySQL } from '@codemirror/lang-sql';
import { oneDark } from '@codemirror/theme-one-dark';
import { defaultKeymap } from '@codemirror/commands';
import { useTheme } from '../../hooks/useTheme';

import { Driver } from '../../lib/drivers';

interface SQLEditorProps {
  value: string;
  onChange: (value: string) => void;
  onExecute?: () => void;
  onExecuteSelection?: (selection: string) => void;
  dialect?: Driver;
  readOnly?: boolean;
}

export function SQLEditor({
  value,
  onChange,
  onExecute,
  onExecuteSelection,
  dialect = 'postgres',
  readOnly = false,
}: SQLEditorProps) {
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const { isDark } = useTheme();

  // Get SQL dialect
  const sqlDialect = useMemo(() => {
    switch (dialect) {
      case 'mysql':
        return MySQL;
      case 'postgres':
      default:
        return PostgreSQL;
    }
  }, [dialect]);

  useEffect(() => {
    if (!editorRef.current) return;

    // Custom keymap for execute
    const executeKeymap = keymap.of([
      {
        key: 'Mod-Enter',
        run: view => {
          const selection = view.state.sliceDoc(
            view.state.selection.main.from,
            view.state.selection.main.to
          );

          if (selection && onExecuteSelection) {
            onExecuteSelection(selection);
          } else if (onExecute) {
            onExecute();
          }
          return true;
        },
      },
    ]);

    const extensions = [
      lineNumbers(),
      highlightActiveLine(),
      sql({ dialect: sqlDialect }),
      executeKeymap,
      keymap.of(defaultKeymap),
      EditorView.updateListener.of(update => {
        if (update.docChanged) {
          onChange(update.state.doc.toString());
        }
      }),
      EditorView.editable.of(!readOnly),
      EditorView.theme({
        '&': { height: '100%' },
        '.cm-scroller': { overflow: 'auto' },
      }),
    ];

    // Add dark theme if needed
    if (isDark) {
      extensions.push(oneDark);
    }

    const state = EditorState.create({
      doc: value,
      extensions,
    });

    const view = new EditorView({
      state,
      parent: editorRef.current,
    });

    viewRef.current = view;

    return () => {
      view.destroy();
    };
  }, [isDark, sqlDialect, onChange, onExecute, onExecuteSelection, readOnly, value]); // Recreate when relevant inputs change

  // Sync external value changes
  useEffect(() => {
    const view = viewRef.current;
    if (view && value !== view.state.doc.toString()) {
      view.dispatch({
        changes: {
          from: 0,
          to: view.state.doc.length,
          insert: value,
        },
      });
    }
  }, [value]);

  return <div className="flex-1 overflow-hidden h-full text-base" ref={editorRef} />;
}
