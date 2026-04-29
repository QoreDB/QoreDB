// SPDX-License-Identifier: Apache-2.0

import { autocompletion } from '@codemirror/autocomplete';
import { defaultKeymap } from '@codemirror/commands';
import { json } from '@codemirror/lang-json';
import { EditorState } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import { EditorView, highlightActiveLine, keymap, lineNumbers } from '@codemirror/view';
import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useTheme } from '../../hooks/useTheme';
import { useSchemaCache } from '../../hooks/useSchemaCache';
import type { Namespace } from '../../lib/tauri';
import { createMongoCompletionSource } from './extensions/mongoCompletion';
import { mongoOperatorHighlight } from './extensions/mongoHighlight';
import { mongoLinter } from './extensions/mongoLint';

interface MongoEditorProps {
  value: string;
  onChange: (value: string) => void;
  onExecute?: () => void;
  readOnly?: boolean;
  sessionId?: string | null;
  activeNamespace?: Namespace | null;
}

export function MongoEditor({
  value,
  onChange,
  onExecute,
  readOnly = false,
  sessionId,
  activeNamespace,
}: MongoEditorProps) {
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const initialValueRef = useRef(value);
  const onChangeRef = useRef(onChange);
  const onExecuteRef = useRef(onExecute);
  const sessionIdRef = useRef(sessionId);
  const namespaceRef = useRef(activeNamespace);
  const { isDark } = useTheme();
  const { t } = useTranslation();
  const schemaCache = useSchemaCache(sessionId || '');
  const schemaCacheRef = useRef(schemaCache);
  schemaCacheRef.current = schemaCache;

  useEffect(() => {
    onChangeRef.current = onChange;
    onExecuteRef.current = onExecute;
    sessionIdRef.current = sessionId;
    namespaceRef.current = activeNamespace;
  }, [onChange, onExecute, sessionId, activeNamespace]);

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

    const completionSource = createMongoCompletionSource({
      getSessionId: () => sessionIdRef.current,
      getNamespace: () => namespaceRef.current,
      getSchemaCache: () => schemaCacheRef.current,
    });

    const extensions = [
      lineNumbers(),
      highlightActiveLine(),
      json(),
      mongoOperatorHighlight,
      mongoLinter({
        getErrorMessage: raw => t('mongo.jsonError', { message: raw }),
      }),
      autocompletion({
        override: [completionSource],
        activateOnTyping: true,
        closeOnBlur: true,
      }),
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

    extensions.push(
      EditorView.theme({
        '&': {
          height: '100%',
          ...(isDark ? { backgroundColor: 'var(--q-bg-1)' } : {}),
        },
        '.cm-scroller': { overflow: 'auto' },
        // Highlight MongoDB operators (`$foo`) — applied on top of the
        // JSON grammar which keeps them inside property strings.
        '.cm-mongo-operator': {
          color: isDark ? '#d19a66' : '#af00db',
          fontWeight: '500',
        },
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
  }, [isDark, readOnly, t]);

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
