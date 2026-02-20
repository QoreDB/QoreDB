// SPDX-License-Identifier: Apache-2.0

import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
  snippet,
  snippetCompletion,
} from '@codemirror/autocomplete';
import { defaultKeymap } from '@codemirror/commands';
import { keywordCompletionSource, MySQL, PostgreSQL, sql } from '@codemirror/lang-sql';
import { EditorState } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import {
  EditorView,
  highlightActiveLine,
  keymap,
  lineNumbers,
  placeholder,
} from '@codemirror/view';
/* eslint-disable no-useless-escape */
import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef } from 'react';
import { useSchemaCache } from '../../hooks/useSchemaCache';
import { useTheme } from '../../hooks/useTheme';
import { Driver } from '../../lib/drivers';
import { SQL_SNIPPETS } from '../../lib/sqlSnippets';
import type { Collection, Namespace } from '../../lib/tauri';

interface SQLEditorProps {
  value: string;
  onChange: (value: string) => void;
  onExecute?: () => void;
  onExecuteSelection?: (selection: string) => void;
  onFormat?: () => void;
  dialect?: Driver;
  readOnly?: boolean;
  sessionId?: string | null;
  connectionDatabase?: string;
  activeNamespace?: Namespace | null;
  placeholder?: string;
}

export interface SQLEditorHandle {
  insertSnippet: (snippetText: string) => void;
  getSelection: () => string;
  focus: () => void;
}

interface SchemaState {
  namespaces: Namespace[];
  defaultNamespace: Namespace | null;
  tablesByNamespace: Map<string, Collection[]>;
  columnsByTable: Map<string, string[]>;
}

function createSchemaState(): SchemaState {
  return {
    namespaces: [],
    defaultNamespace: null,
    tablesByNamespace: new Map(),
    columnsByTable: new Map(),
  };
}

function getNamespaceKey(ns: Namespace): string {
  return `${ns.database}:${ns.schema || ''}`;
}

function getTableKey(ns: Namespace, tableName: string): string {
  return `${ns.database}:${ns.schema || ''}:${tableName}`;
}

export const SQLEditor = forwardRef<SQLEditorHandle, SQLEditorProps>(function SQLEditor(
  {
    value,
    onChange,
    onExecute,
    onExecuteSelection,
    onFormat,
    dialect = Driver.Postgres,
    readOnly = false,
    sessionId,
    connectionDatabase,
    activeNamespace,
    placeholder: placeholderProp,
  },
  ref
) {
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const initialValueRef = useRef(value);
  const onChangeRef = useRef(onChange);
  const onExecuteRef = useRef(onExecute);
  const onExecuteSelectionRef = useRef(onExecuteSelection);
  const onFormatRef = useRef(onFormat);
  const readOnlyRef = useRef(readOnly);
  const { isDark } = useTheme();
  const schemaCache = useSchemaCache(sessionId || '');
  const schemaStateRef = useRef<SchemaState>(createSchemaState());

  const sqlDialect = useMemo(() => {
    switch (dialect) {
      case Driver.Mysql:
        return MySQL;
      default:
        return PostgreSQL;
    }
  }, [dialect]);

  const keywordSource = useMemo(() => keywordCompletionSource(sqlDialect, true), [sqlDialect]);

  const snippetCompletions = useMemo(
    () =>
      SQL_SNIPPETS.map(snippetDef =>
        snippetCompletion(snippetDef.template, {
          label: snippetDef.label,
          detail: snippetDef.description,
          type: 'keyword',
        })
      ),
    []
  );

  const resolveDefaultNamespace = useCallback(
    (namespaces: Namespace[]): Namespace | null => {
      if (!namespaces.length) return null;
      if (activeNamespace) {
        const match = namespaces.find(
          ns =>
            ns.database === activeNamespace.database &&
            (ns.schema || '') === (activeNamespace.schema || '')
        );
        if (match) return match;
      }
      if (!connectionDatabase) return namespaces[0];
      const matches = namespaces.filter(ns => ns.database === connectionDatabase);
      if (!matches.length) return namespaces[0];
      const publicMatch = matches.find(ns => ns.schema === 'public');
      return publicMatch || matches[0];
    },
    [activeNamespace, connectionDatabase]
  );

  const loadTablesForNamespace = useCallback(
    async (ns: Namespace): Promise<Collection[]> => {
      const key = getNamespaceKey(ns);
      const cached = schemaStateRef.current.tablesByNamespace.get(key);
      if (cached) return cached;
      const collections = await schemaCache.getCollections(ns);
      schemaStateRef.current.tablesByNamespace.set(key, collections);
      return collections;
    },
    [schemaCache]
  );

  const loadNamespaces = useCallback(async () => {
    if (!sessionId) return;
    const namespaces = await schemaCache.getNamespaces();
    schemaStateRef.current.namespaces = namespaces;
    schemaStateRef.current.defaultNamespace = resolveDefaultNamespace(namespaces);
    const defaultNs = schemaStateRef.current.defaultNamespace;
    if (defaultNs) {
      await loadTablesForNamespace(defaultNs);
    }
  }, [schemaCache, sessionId, resolveDefaultNamespace, loadTablesForNamespace]);

  const loadColumnsForTable = useCallback(
    async (ns: Namespace, tableName: string): Promise<string[]> => {
      const key = getTableKey(ns, tableName);
      const cached = schemaStateRef.current.columnsByTable.get(key);
      if (cached) return cached;
      const schema = await schemaCache.getTableSchema(ns, tableName);
      const columns = schema?.columns?.map(column => column.name) || [];
      schemaStateRef.current.columnsByTable.set(key, columns);
      return columns;
    },
    [schemaCache]
  );

  const resolveNamespace = useCallback(
    (schemaName?: string): Namespace | null => {
      if (!schemaName) return schemaStateRef.current.defaultNamespace;
      const normalized = schemaName.toLowerCase();
      const currentDatabase = activeNamespace?.database || connectionDatabase;
      const candidates = schemaStateRef.current.namespaces.filter(
        ns =>
          (ns.schema?.toLowerCase() === normalized ||
            (!ns.schema && ns.database.toLowerCase() === normalized)) &&
          (!currentDatabase || ns.database === currentDatabase)
      );
      return candidates[0] || null;
    },
    [activeNamespace, connectionDatabase]
  );

  const completionSource = useCallback(
    async (context: CompletionContext): Promise<CompletionResult | null> => {
      if (!sessionId) return null;
      const word = context.matchBefore(/[\w."]+/);
      if (!word || (word.from === word.to && !context.explicit)) return null;

      const text = word.text.replace(/"/g, '');
      const dotIndex = text.lastIndexOf('.');
      const tableOptions: Completion[] = [];
      const schemaOptions: Completion[] = [];
      let from = word.from;
      const to = word.to;

      if (dotIndex >= 0) {
        const prefix = text.slice(0, dotIndex);
        from = word.from + dotIndex + 1;
        const parts = prefix.split('.');
        if (parts.length === 1) {
          const schemaMatch = resolveNamespace(parts[0]);
          if (
            schemaMatch &&
            (schemaMatch.schema?.toLowerCase() === parts[0].toLowerCase() ||
              (!schemaMatch.schema &&
                schemaMatch.database.toLowerCase() === parts[0].toLowerCase()))
          ) {
            const tables = await loadTablesForNamespace(schemaMatch);
            return {
              from,
              to,
              options: tables.map(table => ({
                label: table.name,
                type: table.collection_type === 'View' ? 'view' : 'table',
              })),
              validFor: /[\w$"]*/,
            };
          }

          const tableNamespace = schemaStateRef.current.defaultNamespace;
          if (!tableNamespace) return null;
          const columns = await loadColumnsForTable(tableNamespace, parts[0]);
          return {
            from,
            to,
            options: columns.map(column => ({ label: column, type: 'property' })),
            validFor: /[\w$"]*/,
          };
        }

        const tableName = parts[parts.length - 1];
        const schemaName = parts[parts.length - 2];
        const tableNamespace = resolveNamespace(schemaName);
        if (!tableNamespace) return null;
        const columns = await loadColumnsForTable(tableNamespace, tableName);
        return {
          from,
          to,
          options: columns.map(column => ({ label: column, type: 'property' })),
          validFor: /[\w$"]*/,
        };
      }

      const namespaces = schemaStateRef.current.namespaces;
      const seenSchemas = new Set<string>();
      for (const ns of namespaces) {
        const schemaLabel = ns.schema || ns.database;
        if (schemaLabel) {
          const normalized = schemaLabel.toLowerCase();
          if (seenSchemas.has(normalized)) continue;
          seenSchemas.add(normalized);
          schemaOptions.push({
            label: schemaLabel,
            type: 'namespace',
            detail: ns.database,
          });
        }
      }

      const defaultNs = schemaStateRef.current.defaultNamespace;
      if (defaultNs) {
        const tables = await loadTablesForNamespace(defaultNs);
        const shouldQualify =
          dialect === Driver.Mysql &&
          ((!!connectionDatabase && connectionDatabase !== defaultNs.database) ||
            (!connectionDatabase && !!defaultNs.database));
        const qualifyTable = (tableName: string) => {
          if (!shouldQualify) return tableName;
          const dbName = defaultNs.database.replace(/`/g, '``');
          const table = tableName.replace(/`/g, '``');
          return `\`${dbName}\`.\`${table}\``;
        };
        tableOptions.push(
          ...tables.map(table => ({
            label: table.name,
            type: table.collection_type === 'View' ? 'view' : 'table',
            detail: defaultNs.schema ? defaultNs.schema : defaultNs.database,
            apply: qualifyTable(table.name),
          }))
        );
      }

      const keywordResult = await keywordSource(context);
      const options: Completion[] = [
        ...snippetCompletions,
        ...schemaOptions,
        ...tableOptions,
        ...(keywordResult?.options || []),
      ];

      return {
        from,
        to,
        options,
        validFor: /[\w$"]*/,
      };
    },
    [
      dialect,
      connectionDatabase,
      keywordSource,
      loadColumnsForTable,
      loadTablesForNamespace,
      resolveNamespace,
      sessionId,
      snippetCompletions,
    ]
  );

  useEffect(() => {
    onChangeRef.current = onChange;
    onExecuteRef.current = onExecute;
    onExecuteSelectionRef.current = onExecuteSelection;
    onFormatRef.current = onFormat;
    readOnlyRef.current = readOnly;
  }, [onChange, onExecute, onExecuteSelection, onFormat, readOnly]);

  useEffect(() => {
    schemaStateRef.current = createSchemaState();
    if (sessionId) {
      void loadNamespaces();
    }
  }, [sessionId, loadNamespaces]);

  useImperativeHandle(
    ref,
    () => ({
      insertSnippet: snippetText => {
        const view = viewRef.current;
        if (!view || readOnlyRef.current) return;
        const { from, to } = view.state.selection.main;
        snippet(snippetText)(view, null, from, to);
        view.focus();
      },
      getSelection: () => {
        const view = viewRef.current;
        if (!view) return '';
        const { from, to } = view.state.selection.main;
        return view.state.sliceDoc(from, to);
      },
      focus: () => {
        viewRef.current?.focus();
      },
    }),
    []
  );

  useEffect(() => {
    if (!editorRef.current) return;

    const executeKeymap = keymap.of([
      {
        key: 'Mod-Enter',
        run: view => {
          const selection = view.state.sliceDoc(
            view.state.selection.main.from,
            view.state.selection.main.to
          );

          if (selection && onExecuteSelectionRef.current) {
            onExecuteSelectionRef.current(selection);
          } else if (onExecuteRef.current) {
            onExecuteRef.current();
          }
          return true;
        },
      },
      {
        key: 'Shift-Alt-F',
        run: () => {
          if (onFormatRef.current) {
            onFormatRef.current();
            return true;
          }
          return false;
        },
      },
      {
        key: 'Mod-Shift-F',
        run: () => {
          if (onFormatRef.current) {
            onFormatRef.current();
            return true;
          }
          return false;
        },
      },
    ]);

    const extensions = [
      lineNumbers(),
      highlightActiveLine(),
      sql({ dialect: sqlDialect }),
      ...(placeholderProp ? [placeholder(placeholderProp)] : []),
      autocompletion({
        activateOnTyping: true,
        override: [completionSource],
      }),
      executeKeymap,
      keymap.of(defaultKeymap),
      EditorView.updateListener.of(update => {
        if (update.docChanged) {
          onChangeRef.current(update.state.doc.toString());
        }
      }),
      EditorView.editable.of(!readOnly),
      EditorView.theme({
        '&': { height: '100%' },
        '.cm-scroller': { overflow: 'auto' },
      }),
    ];

    if (isDark) {
      extensions.push(oneDark);
    }

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
  }, [isDark, sqlDialect, readOnly, completionSource]);

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

  return <div className="flex-1 overflow-hidden h-50 text-base" ref={editorRef} />;
});
