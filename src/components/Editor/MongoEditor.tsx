import { useEffect, useRef } from 'react';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view';
import { json } from '@codemirror/lang-json';
import { oneDark } from '@codemirror/theme-one-dark';
import { defaultKeymap } from '@codemirror/commands';
import { useTheme } from '../../hooks/useTheme';

interface MongoEditorProps {
  value: string;
  onChange: (value: string) => void;
  onExecute?: () => void;
  readOnly?: boolean;
}

// Template queries for MongoDB
export const MONGO_TEMPLATES = {
  find: `db.collection.find({
  // query filter
})`,
  findOne: `db.collection.findOne({
  // query filter
})`,
  aggregate: `db.collection.aggregate([
  { $match: { } },
  { $group: { _id: "$field", count: { $sum: 1 } } }
])`,
  insertOne: `db.collection.insertOne({
  // document
})`,
  updateOne: `db.collection.updateOne(
  { /* filter */ },
  { $set: { /* update */ } }
)`,
  updateMany: `db.collection.updateMany(
  { /* filter */ },
  { $set: { /* update */ } }
)`,
  deleteOne: `db.collection.deleteOne({
  // filter
})`,
};

export function MongoEditor({
  value,
  onChange,
  onExecute,
  readOnly = false,
}: MongoEditorProps) {
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const { isDark } = useTheme();

  useEffect(() => {
    if (!editorRef.current) return;

    const executeKeymap = keymap.of([
      {
        key: 'Mod-Enter',
        run: () => {
          onExecute?.();
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
          onChange(update.state.doc.toString());
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
  }, [isDark, onChange, onExecute, readOnly, value]);

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
    <div className="flex-1 overflow-hidden h-full text-base" ref={editorRef} />
  );
}
