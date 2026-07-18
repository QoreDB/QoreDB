// SPDX-License-Identifier: BUSL-1.1

import { AlertCircle, Bot } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Markdown } from '@/components/ui/markdown';
import type { AgentChatItem } from '@/hooks/useAgentChat';
import { PermissionCard } from './PermissionCard';
import { ToolStepCard } from './ToolStepCard';

interface AgentThreadProps {
  items: AgentChatItem[];
  loading: boolean;
  onRespondPermission: (permissionId: string, approved: boolean, remember: boolean) => void;
}

export function AgentThread({ items, loading, onRespondPermission }: AgentThreadProps) {
  const { t } = useTranslation();
  const bottomRef = useRef<HTMLDivElement>(null);
  const lastItem = items[items.length - 1];
  const lastAssistantContent = lastItem?.kind === 'assistant' ? lastItem.content : undefined;
  const lastToolResolved = lastItem?.kind === 'tool' ? Boolean(lastItem.result) : undefined;

  // biome-ignore lint/correctness/useExhaustiveDependencies: re-scroll when the thread grows or the last item changes, not when captured values change
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: 'end' });
  }, [items.length, lastAssistantContent, lastToolResolved]);

  if (items.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground">
        <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-accent/10 text-[var(--q-accent)]">
          <Bot size={24} />
        </div>
        <p className="text-sm font-medium text-foreground">{t('agentChat.emptyState')}</p>
        <p className="max-w-sm text-center text-xs leading-relaxed">{t('agentChat.emptyHint')}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      {items.map(item => {
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
              return (
                <div
                  key={item.id}
                  className="flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive/5 p-3 text-sm"
                >
                  <AlertCircle size={15} className="mt-0.5 shrink-0 text-destructive" />
                  <span className="min-w-0 break-words">{item.error}</span>
                </div>
              );
            }
            if (!item.content && item.streaming) {
              return (
                <div key={item.id} className="text-sm text-muted-foreground animate-pulse">
                  {t('agentChat.thinking')}
                </div>
              );
            }
            if (!item.content) return null;
            return (
              <div key={item.id} className="max-w-full text-sm leading-relaxed">
                <Markdown>{item.content}</Markdown>
              </div>
            );
          case 'tool':
            return <ToolStepCard key={item.id} item={item} />;
          case 'permission':
            return <PermissionCard key={item.id} item={item} onRespond={onRespondPermission} />;
          default:
            return null;
        }
      })}
      {loading && lastItem?.kind !== 'assistant' && lastItem?.kind !== 'permission' && (
        <div className="text-sm text-muted-foreground animate-pulse">{t('agentChat.thinking')}</div>
      )}
      <div ref={bottomRef} />
    </div>
  );
}
