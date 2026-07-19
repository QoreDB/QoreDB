// SPDX-License-Identifier: BUSL-1.1

import { AlertCircle, CheckCircle2, ChevronDown, ChevronRight, Loader2 } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { QoreAiMark } from '@/components/Brand/QoreAiMark';
import type { AgentChatItem } from '@/hooks/useAgentChat';
import { ToolStepCard } from './ToolStepCard';

type ToolItem = Extract<AgentChatItem, { kind: 'tool' }>;

interface AgentActivityGroupProps {
  items: ToolItem[];
  active: boolean;
}

/** A compact disclosure for the agent's observable tool activity. */
export function AgentActivityGroup({ items, active }: AgentActivityGroupProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const hasError = items.some(item => item.result?.isError);
  const latest = items[items.length - 1];

  return (
    <div className="overflow-hidden rounded-lg border border-border bg-muted/20">
      <button
        type="button"
        aria-expanded={expanded}
        onClick={() => setExpanded(value => !value)}
        className="flex min-h-9 w-full items-center gap-2 px-3 py-2 text-left text-xs transition-colors hover:bg-muted/40"
      >
        <QoreAiMark compact size={15} className={active ? 'animate-pulse' : undefined} />
        <span className="shrink-0 font-medium text-foreground">
          {active ? t('agentChat.activity.inProgress') : t('agentChat.activity.complete')}
        </span>
        {!expanded && latest && (
          <span className="min-w-0 truncate font-mono text-[11px] text-muted-foreground">
            {latest.name}
          </span>
        )}
        <span className="ml-auto shrink-0 text-muted-foreground">
          {t('agentChat.activity.steps', { count: items.length })}
        </span>
        {active ? (
          <Loader2 size={13} className="shrink-0 animate-spin text-[var(--q-ai-signal)]" />
        ) : hasError ? (
          <AlertCircle size={13} className="shrink-0 text-destructive" />
        ) : (
          <CheckCircle2 size={13} className="shrink-0 text-[var(--q-ai-verified)]" />
        )}
        {expanded ? (
          <ChevronDown size={14} className="shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight size={14} className="shrink-0 text-muted-foreground" />
        )}
      </button>
      {expanded && (
        <div className="space-y-2 border-t border-border p-2">
          {items.map(item => (
            <ToolStepCard key={item.id} item={item} />
          ))}
        </div>
      )}
    </div>
  );
}
