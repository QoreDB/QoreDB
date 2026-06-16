// SPDX-License-Identifier: Apache-2.0

/**
 * Console editor for Elasticsearch / OpenSearch ("Dev Tools" style).
 *
 * Format: a first line `METHOD /path` followed by an optional JSON (or NDJSON
 * for `_bulk`) body. The method keyword and path on the first line are
 * highlighted; the JSON body is validated (except for bulk/NDJSON requests).
 * Autocomplete on the method line proposes endpoints and live index names.
 */

import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete';
import { defaultKeymap } from '@codemirror/commands';
import { json } from '@codemirror/lang-json';
import { type Diagnostic, linter } from '@codemirror/lint';
import { EditorState, RangeSetBuilder } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  highlightActiveLine,
  keymap,
  lineNumbers,
  ViewPlugin,
  type ViewUpdate,
} from '@codemirror/view';
import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useSchemaCache } from '../../hooks/useSchemaCache';
import { useTheme } from '../../hooks/useTheme';
import type { Namespace } from '../../lib/tauri';
import { SEARCH_ENDPOINTS, SEARCH_METHODS } from './search-constants';

interface SearchEditorProps {
  value: string;
  onChange: (value: string) => void;
  onExecute?: () => void;
  readOnly?: boolean;
  sessionId?: string | null;
  activeNamespace?: Namespace | null;
}

const METHOD_LINE_RE = /^(\s*)(GET|POST|PUT|DELETE|HEAD)(\s+)(\S.*)?$/i;

/** Highlights the method keyword and path on the first line. */
const methodLineHighlighter = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = this.build(view);
    }

    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) {
        this.decorations = this.build(update.view);
      }
    }

    build(view: EditorView): DecorationSet {
      const builder = new RangeSetBuilder<Decoration>();
      if (view.state.doc.lines >= 1) {
        const line = view.state.doc.line(1);
        const m = METHOD_LINE_RE.exec(line.text);
        if (m) {
          const methodFrom = line.from + m[1].length;
          const methodTo = methodFrom + m[2].length;
          builder.add(methodFrom, methodTo, Decoration.mark({ class: 'cm-search-method' }));
          if (m[4]) {
            const pathFrom = methodTo + m[3].length;
            const pathTo = pathFrom + m[4].length;
            builder.add(pathFrom, pathTo, Decoration.mark({ class: 'cm-search-path' }));
          }
        }
      }
      return builder.finish();
    }
  },
  { decorations: v => v.decorations }
);

/** Validates the JSON body (everything after the first line). Skips NDJSON. */
function searchBodyLinter(getMessage: (raw: string) => string) {
  return linter((view: EditorView): Diagnostic[] => {
    const doc = view.state.doc.toString();
    const nl = doc.indexOf('\n');
    if (nl < 0) return [];

    const firstLine = doc.slice(0, nl);
    const bodyStart = nl + 1;
    const body = doc.slice(bodyStart);
    if (!body.trim()) return [];

    // Bulk / multi-search bodies are NDJSON, not a single JSON document.
    if (/_bulk|_msearch/i.test(firstLine)) return [];

    try {
      JSON.parse(body);
      return [];
    } catch (err) {
      const raw = err instanceof Error ? err.message : String(err);
      return [
        {
          from: bodyStart,
          to: doc.length,
          severity: 'error',
          message: getMessage(raw),
        },
      ];
    }
  });
}

const METHOD_COMPLETIONS: Completion[] = SEARCH_METHODS.map(label => ({
  label,
  type: 'keyword',
}));

const ENDPOINT_COMPLETIONS: Completion[] = SEARCH_ENDPOINTS.map(e => ({
  label: e.label,
  type: 'constant',
  detail: e.detail,
}));

export function SearchEditor({
  value,
  onChange,
  onExecute,
  readOnly = false,
  sessionId,
  activeNamespace,
}: SearchEditorProps) {
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

    async function completionSource(context: CompletionContext): Promise<CompletionResult | null> {
      const line = context.state.doc.lineAt(context.pos);
      // Only complete on the method line (line 1).
      if (line.number !== 1) return null;

      const token = context.matchBefore(/[\w./?=*-]+/);
      const before = context.state.doc.sliceString(line.from, context.pos);

      // At the very start of the line → propose HTTP methods.
      if (!/\s/.test(before)) {
        if (!token && !context.explicit) return null;
        return {
          from: token ? token.from : context.pos,
          to: context.pos,
          options: METHOD_COMPLETIONS,
          validFor: /^[A-Za-z]*$/,
        };
      }

      // After the method → propose endpoints + live index names.
      const options: Completion[] = [...ENDPOINT_COMPLETIONS];
      try {
        const ns = namespaceRef.current;
        if (ns) {
          const collections = await schemaCacheRef.current.getCollections(ns);
          for (const c of collections) {
            options.push({
              label: c.name,
              type: c.collection_type === 'View' ? 'view' : 'class',
            });
          }
        }
      } catch {
        // ignore — endpoints alone are still useful
      }

      return {
        from: token ? token.from : context.pos,
        to: context.pos,
        options,
        validFor: /^[\w./?=*-]*$/,
      };
    }

    const extensions = [
      lineNumbers(),
      highlightActiveLine(),
      json(),
      methodLineHighlighter,
      searchBodyLinter(raw => t('search.jsonError', { message: raw })),
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
        '.cm-search-method': {
          color: isDark ? '#61afef' : '#0550ae',
          fontWeight: '600',
        },
        '.cm-search-path': {
          color: isDark ? '#98c379' : '#0a7d22',
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
