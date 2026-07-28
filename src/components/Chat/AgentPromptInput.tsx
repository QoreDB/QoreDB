// SPDX-License-Identifier: BUSL-1.1

import { ArrowUp, Square } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';

interface AgentPromptInputProps {
  onSubmit: (prompt: string) => void;
  onCancel: () => void;
  loading: boolean;
  disabled?: boolean;
}

const MAX_HEIGHT_PX = 200;

export function AgentPromptInput({ onSubmit, onCancel, loading, disabled }: AgentPromptInputProps) {
  const { t } = useTranslation();
  const [prompt, setPrompt] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (!disabled) textareaRef.current?.focus();
  }, [disabled]);

  const autoResize = useCallback(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, MAX_HEIGHT_PX)}px`;
  }, []);

  const handleSubmit = useCallback(() => {
    const trimmed = prompt.trim();
    if (!trimmed || loading || disabled) return;
    onSubmit(trimmed);
    setPrompt('');
    requestAnimationFrame(() => {
      const el = textareaRef.current;
      if (el) el.style.height = 'auto';
    });
  }, [prompt, loading, disabled, onSubmit]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit]
  );

  return (
    <div className="w-full border-t border-border bg-background/95 px-5 py-3 backdrop-blur-sm">
      <div className="mx-auto flex w-full max-w-4xl items-end gap-1.5 rounded-xl border border-border bg-muted/30 p-1.5 shadow-sm transition-colors focus-within:border-[var(--q-accent)]">
        <textarea
          ref={textareaRef}
          value={prompt}
          onChange={e => {
            setPrompt(e.target.value);
            autoResize();
          }}
          onKeyDown={handleKeyDown}
          placeholder={t('agentChat.promptPlaceholder')}
          disabled={disabled}
          rows={1}
          className="max-h-[200px] flex-1 resize-none bg-transparent px-2.5 py-2 text-sm outline-none placeholder:text-muted-foreground disabled:opacity-50"
        />
        {loading ? (
          <Button
            size="icon"
            variant="outline"
            onClick={onCancel}
            className="h-8 w-8 shrink-0 rounded-md"
            title={t('agentChat.cancel')}
          >
            <Square size={13} />
          </Button>
        ) : (
          <Button
            size="icon"
            onClick={handleSubmit}
            disabled={!prompt.trim() || disabled}
            className="h-8 w-8 shrink-0 rounded-md"
            title={t('agentChat.send')}
          >
            <ArrowUp size={15} />
          </Button>
        )}
      </div>
    </div>
  );
}
