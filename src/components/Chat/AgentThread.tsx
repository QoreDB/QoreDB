// SPDX-License-Identifier: BUSL-1.1

import { AlertCircle } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { QoreAiMark, QoreAiMonoMark } from '@/components/Brand/QoreAiMark';
import { Markdown } from '@/components/ui/markdown';
import type { AgentChatItem } from '@/hooks/useAgentChat';
import type { AgentUsage } from '@/lib/agent';
import { AgentActivityGroup } from './AgentActivityGroup';
import { PermissionCard } from './PermissionCard';

export function usageTotalTokens(usage: AgentUsage): number {
  return (
    (usage.input_tokens ?? 0) +
    (usage.output_tokens ?? 0) +
    (usage.cache_read_tokens ?? 0) +
    (usage.cache_creation_tokens ?? 0)
  );
}

function UsageLine({ usage, iterations }: { usage: AgentUsage; iterations?: number }) {
  const { t, i18n } = useTranslation();
  const total = usageTotalTokens(usage);
  if (total === 0) return null;
  const format = new Intl.NumberFormat(i18n.language, {
    notation: 'compact',
    maximumFractionDigits: 1,
  });
  const parts = [t('agentChat.usage.tokens', { value: format.format(total) })];
  if (iterations && iterations > 1) {
    parts.push(t('agentChat.usage.iterations', { count: iterations }));
  }
  const cacheRead = usage.cache_read_tokens ?? 0;
  const promptTokens = (usage.input_tokens ?? 0) + cacheRead + (usage.cache_creation_tokens ?? 0);
  if (cacheRead > 0 && promptTokens > 0) {
    parts.push(
      t('agentChat.usage.cached', { percent: Math.round((cacheRead / promptTokens) * 100) })
    );
  }
  return (
    <p className="mt-1.5 select-none text-[11px] text-muted-foreground/70">{parts.join(' · ')}</p>
  );
}

type ToolItem = Extract<AgentChatItem, { kind: 'tool' }>;
type ThreadEntry =
  | Exclude<AgentChatItem, { kind: 'tool' }>
  | { kind: 'activity'; id: string; items: ToolItem[] };

function groupToolActivity(items: AgentChatItem[]): ThreadEntry[] {
  const entries: ThreadEntry[] = [];
  for (const item of items) {
    const previous = entries[entries.length - 1];
    if (item.kind === 'tool' && previous?.kind === 'activity') {
      previous.items.push(item);
    } else if (item.kind === 'tool') {
      entries.push({ kind: 'activity', id: `activity-${item.id}`, items: [item] });
    } else {
      entries.push(item);
    }
  }
  return entries;
}

interface AgentThreadProps {
  items: AgentChatItem[];
  loading: boolean;
  onRespondPermission: (permissionId: string, approved: boolean, remember: boolean) => void;
}

export function AgentThread({ items, loading, onRespondPermission }: AgentThreadProps) {
  const { t } = useTranslation();
  const bottomRef = useRef<HTMLDivElement>(null);
  const lastItem = items[items.length - 1];
  const entries = groupToolActivity(items);
  const lastAssistantContent = lastItem?.kind === 'assistant' ? lastItem.content : undefined;
  const lastToolResolved = lastItem?.kind === 'tool' ? Boolean(lastItem.result) : undefined;

  // biome-ignore lint/correctness/useExhaustiveDependencies: re-scroll when the thread grows or the last item changes, not when captured values change
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: 'end' });
  }, [items.length, lastAssistantContent, lastToolResolved]);

  if (items.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground">
        <div className="flex h-20 w-20 items-center justify-center rounded-2xl bg-accent/10">
          <QoreAiMark size={68} />
        </div>
        <p className="text-sm font-medium text-foreground">{t('agentChat.emptyState')}</p>
        <p className="max-w-sm text-center text-xs leading-relaxed">{t('agentChat.emptyHint')}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      {entries.map((item, index) => {
        switch (item.kind) {
          case 'user':
            return (
              <div key={item.id} className="max-w-[80%] self-end">
                <div className="whitespace-pre-wrap rounded-lg bg-accent/10 px-3.5 py-2.5 text-sm leading-relaxed">
                  {item.content}
                </div>
              </div>
            );
          case 'assistant':
            if (item.error) {
              const isOpenAiPermissionAnomaly =
                item.error.provider === 'OpenAI' &&
                item.error.http_status === 401 &&
                item.error.provider_error_type === 'invalid_request_error' &&
                item.error.message.toLowerCase().includes('insufficient permission');
              const isPermissionError =
                !isOpenAiPermissionAnomaly &&
                (item.error.http_status === 403 ||
                  item.error.provider_error_type?.toLowerCase().includes('permission') ||
                  item.error.provider_code?.toLowerCase().includes('permission'));
              const diagnostics = [
                item.error.provider,
                item.error.http_status ? `HTTP ${item.error.http_status}` : null,
                item.error.provider_code ? `code: ${item.error.provider_code}` : null,
                item.error.provider_error_type ? `type: ${item.error.provider_error_type}` : null,
              ].filter(Boolean);
              return (
                <div
                  key={item.id}
                  className="flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive/5 p-3 text-sm"
                >
                  <AlertCircle size={15} className="mt-0.5 shrink-0 text-destructive" />
                  <div className="min-w-0 space-y-1.5 break-words">
                    <p>{item.error.message}</p>
                    {diagnostics.length > 0 && (
                      <p className="font-mono text-xs text-muted-foreground">
                        {diagnostics.join(' · ')}
                      </p>
                    )}
                    {isPermissionError && (
                      <p className="text-xs leading-relaxed text-muted-foreground">
                        {t('agentChat.error.forbiddenHint')}
                      </p>
                    )}
                    {isOpenAiPermissionAnomaly && (
                      <p className="text-xs leading-relaxed text-muted-foreground">
                        {t('agentChat.error.openAiRequestHint')}
                      </p>
                    )}
                    {item.error.request_id && (
                      <p className="select-text font-mono text-xs text-muted-foreground">
                        {t('agentChat.error.requestId')}: {item.error.request_id}
                      </p>
                    )}
                  </div>
                </div>
              );
            }
            if (!item.content && item.streaming) {
              return (
                <div
                  key={item.id}
                  className="flex items-center gap-2 text-sm text-muted-foreground animate-pulse"
                >
                  <QoreAiMonoMark size={16} />
                  {t('agentChat.thinking')}
                </div>
              );
            }
            if (!item.content) return null;
            return (
              <div key={item.id} className="max-w-full text-sm leading-relaxed">
                <Markdown>{item.content}</Markdown>
                {!item.streaming && item.usage && (
                  <UsageLine usage={item.usage} iterations={item.iterations} />
                )}
              </div>
            );
          case 'activity':
            return (
              <AgentActivityGroup
                key={item.id}
                items={item.items}
                active={loading && index === entries.length - 1}
              />
            );
          case 'permission':
            return <PermissionCard key={item.id} item={item} onRespond={onRespondPermission} />;
          default:
            return null;
        }
      })}
      {loading &&
        lastItem?.kind !== 'assistant' &&
        lastItem?.kind !== 'permission' &&
        lastItem?.kind !== 'tool' && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground animate-pulse">
            <QoreAiMonoMark size={16} />
            {t('agentChat.thinking')}
          </div>
        )}
      <div ref={bottomRef} />
    </div>
  );
}
