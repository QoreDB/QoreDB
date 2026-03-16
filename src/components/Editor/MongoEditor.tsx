// SPDX-License-Identifier: Apache-2.0

import { defaultKeymap } from '@codemirror/commands';
import { json } from '@codemirror/lang-json';
import { EditorState } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import { EditorView, highlightActiveLine, keymap, lineNumbers } from '@codemirror/view';
import { useEffect, useRef } from 'react';
import { useTheme } from '../../hooks/useTheme';

interface MongoEditorProps {
  value: string;
  onChange: (value: string) => void;
  onExecute?: () => void;
  readOnly?: boolean;
}

export function MongoEditor({ value, onChange, onExecute, readOnly = false }: MongoEditorProps) {
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const initialValueRef = useRef(value);
  const onChangeRef = useRef(onChange);
  const onExecuteRef = useRef(onExecute);
  const { isDark } = useTheme();

  useEffect(() => {
    onChangeRef.current = onChange;
    onExecuteRef.current = onExecute;
  }, [onChange, onExecute]);

  useEffect(() => {
    if (!editorRef.current) return;

    const executeKeymap = keymap.of([
      {
        key: 'Mod-Enter',
        run: () => {
          onExecuteRef.current?.();
          return true;
        },
      },
    ]);

    const extensions = [
      lineNumbers(),
      highlightActiveLine(),
      json(),
      executeKeymap,
      keymap.of(defaultKeymap),
      EditorView.updateListener.of(update => {
        if (update.docChanged) {
          onChangeRef.current(update.state.doc.toString());
        }
      }),
      EditorView.editable.of(!readOnly),
    ];

    if (isDark) {
      extensions.push(oneDark);
    }

    // Custom theme applied last so it overrides oneDark background
    extensions.push(
      EditorView.theme({
        '&': {
          height: '100%',
          ...(isDark ? { backgroundColor: 'var(--q-bg-1)' } : {}),
        },
        '.cm-scroller': { overflow: 'auto' },
        ...(isDark
          ? {
              '.cm-gutters': {
                backgroundColor: 'var(--q-bg-1)',
                borderRight: '1px solid var(--q-border)',
              },
              '.cm-activeLineGutter': {
                backgroundColor: 'var(--q-bg-2)',
              },
            }
          : {}),
      })
    );

    const state = EditorState.create({
      doc: initialValueRef.current,
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
  }, [isDark, readOnly]);

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
