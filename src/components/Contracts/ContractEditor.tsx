// SPDX-License-Identifier: BUSL-1.1

import { StreamLanguage } from '@codemirror/language';
import { yaml as yamlMode } from '@codemirror/legacy-modes/mode/yaml';
import { EditorState } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import { EditorView, lineNumbers } from '@codemirror/view';
import { AlertTriangle, CheckCircle2, FileCode, Save } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useTheme } from '@/hooks/useTheme';
import { type Contract, ContractParseError, parseContract, saveContract } from '@/lib/contracts';
import { cn } from '@/lib/utils';

interface Props {
  /** Existing source to load. Empty string for a new contract. */
  initialSource: string;
  /** Existing name (filename without extension). Pass an empty string for new. */
  initialName: string;
  /** Locks the name field — used when editing an existing contract. */
  nameLocked: boolean;
  /** Called after a successful save with the parsed contract. */
  onSaved: (contract: Contract, name: string) => void;
  /** Called when the user clicks "Discard"/"Cancel" without saving. */
  onCancel: () => void;
  /** Called whenever the source changes — parent can compare to compute "unsaved". */
  onDirtyChange?: (dirty: boolean) => void;
}

const TEMPLATE_SOURCE = `name: orders_quality
version: 1
description: Example contract for the orders table
target:
  connection: prod-pg
  schema: public
  table: orders
rules:
  - id: id_unique
    type: unique
    columns: [id]
  - id: status_enum
    type: allowed_values
    column: status
    values: [pending, paid, shipped, refunded]
  - id: amount_positive
    type: numeric_range
    column: amount_cents
    min: 0
    inclusive_min: true
`;

type Validation =
  | { status: 'idle' }
  | { status: 'valid'; contract: Contract }
  | { status: 'invalid'; error: string };

export function ContractEditor({
  initialSource,
  initialName,
  nameLocked,
  onSaved,
  onCancel,
  onDirtyChange,
}: Props) {
  const { t } = useTranslation();
  const { resolvedTheme } = useTheme();
  const [name, setName] = useState(initialName);
  const [source, setSource] = useState(initialSource || TEMPLATE_SOURCE);
  const [validation, setValidation] = useState<Validation>({ status: 'idle' });
  const [saving, setSaving] = useState(false);
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);

  // Track dirty state — `initialSource` empty means new contract; always dirty
  // after first user input.
  const baseSource = useMemo(() => initialSource, [initialSource]);
  useEffect(() => {
    onDirtyChange?.(source !== baseSource || name !== initialName);
  }, [source, name, baseSource, initialName, onDirtyChange]);

  // Debounced validation.
  useEffect(() => {
    const handle = window.setTimeout(() => {
      try {
        const contract = parseContract(source);
        setValidation({ status: 'valid', contract });
      } catch (e) {
        const msg = e instanceof ContractParseError ? e.message : String(e);
        setValidation({ status: 'invalid', error: msg });
      }
    }, 300);
    return () => window.clearTimeout(handle);
  }, [source]);

  // Mount CodeMirror once per theme. `source` is intentionally NOT a dep:
  // CodeMirror only reads the doc at construction; we sync it back via the
  // dedicated dispatch effect below to avoid tearing down the editor on
  // every keystroke.
  // biome-ignore lint/correctness/useExhaustiveDependencies: see above
  useEffect(() => {
    if (!editorRef.current) return;
    const view = new EditorView({
      state: EditorState.create({
        doc: source,
        extensions: [
          lineNumbers(),
          StreamLanguage.define(yamlMode),
          EditorView.lineWrapping,
          EditorView.updateListener.of(update => {
            if (update.docChanged) {
              setSource(update.state.doc.toString());
            }
          }),
          ...(resolvedTheme === 'dark' ? [oneDark] : []),
        ],
      }),
      parent: editorRef.current,
    });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [resolvedTheme]);

  // Sync external source updates (e.g. clicking the template button).
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    if (view.state.doc.toString() !== source) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: source },
      });
    }
  }, [source]);

  const canSave =
    validation.status === 'valid' &&
    !saving &&
    name.trim().length > 0 &&
    validation.contract.name === name.trim();

  const nameMismatch = validation.status === 'valid' && validation.contract.name !== name.trim();

  async function handleSave() {
    if (validation.status !== 'valid') return;
    if (validation.contract.name !== name.trim()) return;
    setSaving(true);
    try {
      await saveContract(name.trim(), source);
      toast.success(t('contracts.editor.saved'));
      onSaved(validation.contract, name.trim());
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      toast.error(t('contracts.editor.saveFailed'), { description: msg });
    } finally {
      setSaving(false);
    }
  }

  function handleInsertTemplate() {
    setSource(TEMPLATE_SOURCE);
  }

  return (
    <div className="flex flex-col gap-3 h-full min-h-0">
      <div className="flex flex-wrap items-center gap-3">
        <Input
          placeholder={t('contracts.editor.namePlaceholder')}
          value={name}
          onChange={e => setName(e.target.value)}
          disabled={nameLocked}
          className="max-w-xs"
        />
        <div className="flex-1 min-w-0">
          <ValidationStatus validation={validation} nameMismatch={nameMismatch} />
        </div>
        <Button type="button" variant="ghost" size="sm" onClick={handleInsertTemplate}>
          <FileCode size={14} />
          {t('contracts.editor.template')}
        </Button>
      </div>

      <div
        ref={editorRef}
        className={cn(
          'flex-1 min-h-[300px] overflow-auto rounded-md border border-border bg-background font-mono text-sm',
          '[&_.cm-editor]:h-full [&_.cm-editor]:outline-none'
        )}
      />

      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel} disabled={saving}>
          {t('contracts.cancel')}
        </Button>
        <Button onClick={handleSave} disabled={!canSave}>
          <Save />
          {t('contracts.editor.save')}
        </Button>
      </div>
    </div>
  );
}

function ValidationStatus({
  validation,
  nameMismatch,
}: {
  validation: Validation;
  nameMismatch: boolean;
}) {
  const { t } = useTranslation();
  if (validation.status === 'idle') {
    return (
      <span className="text-xs text-muted-foreground">{t('contracts.editor.validating')}</span>
    );
  }
  if (validation.status === 'valid' && !nameMismatch) {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs text-emerald-600 dark:text-emerald-400">
        <CheckCircle2 size={14} />
        {t('contracts.editor.valid')}
      </span>
    );
  }
  return (
    <span
      className="inline-flex items-center gap-1.5 text-xs text-amber-600 dark:text-amber-400"
      title={
        validation.status === 'invalid' ? validation.error : t('contracts.editor.nameMismatch')
      }
    >
      <AlertTriangle size={14} />
      {nameMismatch
        ? t('contracts.editor.nameMismatch')
        : validation.status === 'invalid'
          ? validation.error.split('\n')[0]
          : t('contracts.editor.invalid')}
    </span>
  );
}
