// SPDX-License-Identifier: Apache-2.0

import { AlertTriangle, RotateCcw, X } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useShortcutBindings } from '@/hooks/useKeyboardShortcuts';
import {
  type ChordModifier,
  clearAllOverrides,
  clearOverride,
  detectConflicts,
  formatChord,
  isSystemReserved,
  type KeyChord,
  normalizeKey,
  setOverride,
  SHORTCUT_DEFINITIONS,
  type ShortcutCategory,
  type ShortcutDefinition,
  type ShortcutId,
} from '@/lib/shortcuts';
import { Button } from '@/components/ui/button';
import { SettingsCard } from '../SettingsCard';

interface KeyboardShortcutsSectionProps {
  searchQuery?: string;
}

const CATEGORY_ORDER: ShortcutCategory[] = ['general', 'tabs', 'editor'];
const CATEGORY_LABEL: Record<ShortcutCategory, string> = {
  general: 'settings.shortcuts.categoryGeneral',
  tabs: 'settings.shortcuts.categoryNavigation',
  editor: 'settings.shortcuts.categoryEditor',
};

function captureChord(event: KeyboardEvent): KeyChord | null {
  const modifiers: ChordModifier[] = [];
  if (event.metaKey || event.ctrlKey) modifiers.push('mod');
  if (event.shiftKey) modifiers.push('shift');
  if (event.altKey) modifiers.push('alt');

  // Ignore pure modifier presses — wait for a real key.
  const key = event.key;
  if (key === 'Meta' || key === 'Control' || key === 'Shift' || key === 'Alt') {
    return null;
  }

  return { modifiers, key: normalizeKey(key) };
}

function ShortcutRecorder({
  current,
  conflictWith,
  onCommit,
  onReset,
  isOverridden,
}: {
  current: KeyChord;
  conflictWith?: ShortcutId[];
  onCommit: (chord: KeyChord) => void;
  onReset: () => void;
  isOverridden: boolean;
}) {
  const { t } = useTranslation();
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!recording) return;

    function handler(e: KeyboardEvent) {
      e.preventDefault();
      e.stopPropagation();

      if (e.key === 'Escape') {
        setRecording(false);
        setError(null);
        return;
      }

      const chord = captureChord(e);
      if (!chord) return;

      if (isSystemReserved(chord)) {
        setError(t('settings.shortcuts.errorSystemReserved'));
        return;
      }

      onCommit(chord);
      setRecording(false);
      setError(null);
    }

    window.addEventListener('keydown', handler, true);
    return () => window.removeEventListener('keydown', handler, true);
  }, [recording, onCommit, t]);

  return (
    <div className="flex items-center gap-2">
      {conflictWith && conflictWith.length > 0 && (
        <span
          className="flex items-center gap-1 text-xs text-amber-500"
          title={t('settings.shortcuts.conflictTooltip')}
        >
          <AlertTriangle size={12} />
        </span>
      )}
      <button
        type="button"
        onClick={() => {
          setRecording(prev => !prev);
          setError(null);
        }}
        className={`inline-flex items-center px-2 py-0.5 text-xs font-mono rounded border min-w-[80px] justify-center transition-colors ${
          recording
            ? 'border-accent bg-accent/10 text-accent animate-pulse'
            : conflictWith && conflictWith.length > 0
              ? 'border-amber-500/40 bg-amber-500/5'
              : 'border-border/50 bg-muted/70 hover:bg-muted'
        }`}
      >
        {recording ? t('settings.shortcuts.recording') : formatChord(current)}
      </button>
      {isOverridden && !recording && (
        <button
          type="button"
          onClick={onReset}
          aria-label={t('settings.shortcuts.resetOne')}
          className="text-muted-foreground hover:text-foreground"
        >
          <RotateCcw size={12} />
        </button>
      )}
      {error && (
        <span className="text-xs text-destructive flex items-center gap-1">
          <X size={12} />
          {error}
        </span>
      )}
    </div>
  );
}

export function KeyboardShortcutsSection({ searchQuery }: KeyboardShortcutsSectionProps) {
  const { t } = useTranslation();
  const bindings = useShortcutBindings();
  const conflicts = useMemo(() => detectConflicts(bindings), [bindings]);

  const grouped = useMemo(() => {
    const map = new Map<ShortcutCategory, ShortcutDefinition[]>();
    for (const def of SHORTCUT_DEFINITIONS) {
      const arr = map.get(def.category) ?? [];
      arr.push(def);
      map.set(def.category, arr);
    }
    return CATEGORY_ORDER.map(category => ({
      category,
      defs: map.get(category) ?? [],
    })).filter(g => g.defs.length > 0);
  }, []);

  const handleCommit = useCallback((id: ShortcutId, chord: KeyChord) => {
    setOverride(id, chord);
  }, []);

  const handleReset = useCallback((id: ShortcutId) => {
    clearOverride(id);
  }, []);

  const handleResetAll = useCallback(() => {
    clearAllOverrides();
  }, []);

  return (
    <>
      <SettingsCard
        id="shortcuts-overview"
        title={t('settings.shortcuts.title')}
        description={t('settings.shortcuts.description')}
        searchQuery={searchQuery}
      >
        <div className="flex items-center justify-between">
          <p className="text-xs text-muted-foreground max-w-md">
            {t('settings.shortcuts.recordHint')}
          </p>
          <Button variant="outline" size="sm" onClick={handleResetAll}>
            <RotateCcw size={14} className="mr-1" />
            {t('settings.shortcuts.resetAll')}
          </Button>
        </div>
      </SettingsCard>

      {grouped.map(({ category, defs }) => (
        <SettingsCard
          key={category}
          id={`shortcuts-${category}`}
          title={t(CATEGORY_LABEL[category])}
          searchQuery={searchQuery}
        >
          <div className="space-y-1">
            {defs.map(def => {
              const current = bindings[def.id];
              const conflictIds = conflicts[def.id];
              const isOverridden =
                JSON.stringify(current) !== JSON.stringify(def.defaultChord);
              return (
                <div key={def.id} className="flex items-center justify-between py-1.5 text-sm">
                  <span className="text-foreground">{t(def.labelKey)}</span>
                  <ShortcutRecorder
                    current={current}
                    conflictWith={conflictIds}
                    isOverridden={isOverridden}
                    onCommit={chord => handleCommit(def.id, chord)}
                    onReset={() => handleReset(def.id)}
                  />
                </div>
              );
            })}
          </div>
        </SettingsCard>
      ))}
    </>
  );
}
