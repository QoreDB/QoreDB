// SPDX-License-Identifier: BUSL-1.1

import { useState, useCallback, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Textarea } from '@/components/ui/textarea';
import { Button } from '@/components/ui/button';
import { Send, Loader2 } from 'lucide-react';

interface AiPromptInputProps {
  onSubmit: (prompt: string) => void;
  loading: boolean;
  disabled?: boolean;
  placeholder?: string;
}

export function AiPromptInput({ onSubmit, loading, disabled, placeholder }: AiPromptInputProps) {
  const { t } = useTranslation();
  const [prompt, setPrompt] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleSubmit = useCallback(() => {
    const trimmed = prompt.trim();
    if (!trimmed || loading || disabled) return;
    onSubmit(trimmed);
    setPrompt('');
  }, [prompt, loading, disabled, onSubmit]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit]
  );

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  return (
    <div className="flex gap-2 items-end">
      <Textarea
        ref={textareaRef}
        value={prompt}
        onChange={e => setPrompt(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder || t('ai.promptPlaceholder')}
        disabled={loading || disabled}
        className="min-h-[60px] max-h-[120px] resize-none text-sm"
        rows={2}
      />
      <Button
        size="icon"
        onClick={handleSubmit}
        disabled={!prompt.trim() || loading || disabled}
        className="h-9 w-9 shrink-0"
      >
        {loading ? <Loader2 size={16} className="animate-spin" /> : <Send size={16} />}
      </Button>
    </div>
  );
}
