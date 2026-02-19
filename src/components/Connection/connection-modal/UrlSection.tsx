// SPDX-License-Identifier: Apache-2.0

import { Check, Loader2, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Input } from '@/components/ui/input';
import { type PartialConnectionConfig, parseConnectionUrl } from '@/lib/tauri';
import { cn } from '@/lib/utils';

import type { ConnectionFormData } from './types';

interface UrlInputProps {
  formData: ConnectionFormData;
  onChange: (field: keyof ConnectionFormData, value: string | number | boolean) => void;
  onParsedConfig: (config: PartialConnectionConfig) => void;
  onParseStatusChange: (isParsed: boolean) => void;
}

const DEBOUNCE_MS = 400;

export function UrlInput({
  formData,
  onChange,
  onParsedConfig,
  onParseStatusChange,
}: UrlInputProps) {
  const { t } = useTranslation();
  const [parsing, setParsing] = useState(false);
  const [parseResult, setParseResult] = useState<'success' | 'error' | null>(null);
  const [parseError, setParseError] = useState<string | null>(null);
  const [parsedPreview, setParsedPreview] = useState<PartialConnectionConfig | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastParsedUrl = useRef<string>('');

  // Notify parent about parse status
  useEffect(() => {
    onParseStatusChange(parseResult === 'success');
  }, [parseResult, onParseStatusChange]);

  // Auto-parse with debounce
  useEffect(() => {
    const url = formData.connectionUrl.trim();

    // Clear previous timeout
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }

    // Reset if empty
    if (!url) {
      setParseResult(null);
      setParseError(null);
      setParsedPreview(null);
      lastParsedUrl.current = '';
      return;
    }

    // Skip if same URL already parsed
    if (url === lastParsedUrl.current && parseResult === 'success') {
      return;
    }

    // Check if it looks like a valid URL (has ://)
    if (!url.includes('://')) {
      setParseResult(null);
      setParseError(null);
      setParsedPreview(null);
      return;
    }

    // Debounce the parse
    debounceRef.current = setTimeout(async () => {
      setParsing(true);
      setParseResult(null);
      setParseError(null);

      try {
        const result = await parseConnectionUrl(url);

        if (result.success && result.config) {
          setParseResult('success');
          setParsedPreview(result.config);
          lastParsedUrl.current = url;
          onParsedConfig(result.config);
        } else {
          setParseResult('error');
          setParseError(result.error || t('connection.url.parseError'));
          setParsedPreview(null);
        }
      } catch (err) {
        setParseResult('error');
        setParseError(err instanceof Error ? err.message : t('common.error'));
        setParsedPreview(null);
      } finally {
        setParsing(false);
      }
    }, DEBOUNCE_MS);

    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
    };
  }, [formData.connectionUrl, onParsedConfig, t]);

  function handleUrlChange(value: string) {
    onChange('connectionUrl', value);
    // Reset parse state when URL changes (will re-parse via effect)
    if (value.trim() !== lastParsedUrl.current) {
      setParseResult(null);
      setParseError(null);
    }
  }

  return (
    <div className="space-y-2">
      {/* URL Input */}
      <div className="relative">
        <Input
          placeholder="postgres://user:password@localhost:5432/mydb"
          value={formData.connectionUrl}
          onChange={e => handleUrlChange(e.target.value)}
          className={cn(
            'font-mono text-sm h-10 pr-10',
            parseResult === 'success' && 'border-success focus-visible:ring-success',
            parseResult === 'error' && 'border-error focus-visible:ring-error'
          )}
        />
        {/* Status indicator */}
        <div className="absolute right-3 top-1/2 -translate-y-1/2">
          {parsing && <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />}
          {!parsing && parseResult === 'success' && <Check className="h-4 w-4 text-success" />}
          {!parsing && parseResult === 'error' && <X className="h-4 w-4 text-error" />}
        </div>
      </div>

      {/* Error message */}
      {parseResult === 'error' && parseError && <p className="text-xs text-error">{parseError}</p>}

      {/* Success - Compact inline preview */}
      {parseResult === 'success' && parsedPreview && (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          {parsedPreview.host && (
            <span>
              <span className="text-muted-foreground/60">host:</span>{' '}
              <span className="text-foreground">{parsedPreview.host}</span>
              {parsedPreview.port && <span className="text-foreground">:{parsedPreview.port}</span>}
            </span>
          )}
          {parsedPreview.username && (
            <span>
              <span className="text-muted-foreground/60">user:</span>{' '}
              <span className="text-foreground">{parsedPreview.username}</span>
            </span>
          )}
          {parsedPreview.password && (
            <span>
              <span className="text-muted-foreground/60">pass:</span>{' '}
              <span className="text-foreground">••••</span>
            </span>
          )}
          {parsedPreview.database && (
            <span>
              <span className="text-muted-foreground/60">db:</span>{' '}
              <span className="text-foreground">{parsedPreview.database}</span>
            </span>
          )}
          {parsedPreview.ssl !== undefined && (
            <span>
              <span className="text-muted-foreground/60">ssl:</span>{' '}
              <span className="text-foreground">{parsedPreview.ssl ? 'on' : 'off'}</span>
            </span>
          )}
        </div>
      )}
    </div>
  );
}
