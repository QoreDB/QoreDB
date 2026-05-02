// SPDX-License-Identifier: Apache-2.0

import {
  autocompletion,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete';
import { defaultKeymap } from '@codemirror/commands';
import { StreamLanguage } from '@codemirror/language';
import { lua } from '@codemirror/legacy-modes/mode/lua';
import { EditorState } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import { EditorView, highlightActiveLine, keymap, lineNumbers } from '@codemirror/view';
import { useEffect, useRef } from 'react';
import { useTheme } from '../../hooks/useTheme';

interface LuaScriptEditorProps {
  value: string;
  onChange: (value: string) => void;
  onExecute?: () => void;
  readOnly?: boolean;
}

const REDIS_LUA_SNIPPETS: Array<{ label: string; detail: string; apply: string }> = [
  {
    label: 'redis.call',
    detail: 'call(cmd, ...) — propagates errors',
    apply: "redis.call('GET', KEYS[1])",
  },
  {
    label: 'redis.pcall',
    detail: 'pcall(cmd, ...) — returns errors as values',
    apply: "redis.pcall('GET', KEYS[1])",
  },
  {
    label: 'KEYS',
    detail: 'Keys array passed to EVAL',
    apply: 'KEYS[1]',
  },
  {
    label: 'ARGV',
    detail: 'Arguments array passed to EVAL',
    apply: 'ARGV[1]',
  },
  {
    label: 'redis.status_reply',
    detail: 'Return a status reply (simple string)',
    apply: "redis.status_reply('OK')",
  },
  {
    label: 'redis.error_reply',
    detail: 'Return an error reply',
    apply: "redis.error_reply('ERR something')",
  },
  {
    label: 'redis.sha1hex',
    detail: 'Compute SHA1 hex digest of a string',
    apply: 'redis.sha1hex(ARGV[1])',
  },
  {
    label: 'cjson.encode',
    detail: 'Encode a Lua table to JSON',
    apply: 'cjson.encode({ key = ARGV[1] })',
  },
  {
    label: 'cjson.decode',
    detail: 'Decode JSON to a Lua table',
    apply: 'cjson.decode(ARGV[1])',
  },
];

function luaCompletionSource(context: CompletionContext): CompletionResult | null {
  const word = context.matchBefore(/[\w.]*/);
  if (!word || (word.from === word.to && !context.explicit)) {
    return null;
  }
  return {
    from: word.from,
    options: REDIS_LUA_SNIPPETS.map(s => ({
      label: s.label,
      type: 'function',
      detail: s.detail,
      apply: s.apply,
    })),
    validFor: /^[\w.]*$/,
  };
}

export function LuaScriptEditor({
  value,
  onChange,
  onExecute,
  readOnly = false,
}: LuaScriptEditorProps) {
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
      StreamLanguage.define(lua),
      autocompletion({
        override: [luaCompletionSource],
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
        '.cm-scroller': { overflow: 'auto', fontFamily: 'ui-monospace, SFMono-Regular, monospace' },
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

  return (
    <div className="h-56 overflow-hidden text-sm border border-border rounded-md" ref={editorRef} />
  );
}
