// SPDX-License-Identifier: BUSL-1.1

import { ShieldAlert } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import type { AgentChatItem } from '@/hooks/useAgentChat';

type PermissionItem = Extract<AgentChatItem, { kind: 'permission' }>;

interface PermissionCardProps {
  item: PermissionItem;
  onRespond: (permissionId: string, approved: boolean, remember: boolean) => void;
}

export function PermissionCard({ item, onRespond }: PermissionCardProps) {
  const { t } = useTranslation();
  const query = (item.input as { query?: string } | undefined)?.query;

  return (
    <div className="rounded-lg border border-[var(--q-warning)]/40 bg-[var(--q-warning)]/5 p-3 text-sm">
      <div className="flex items-center gap-2">
        <ShieldAlert size={15} className="shrink-0 text-[var(--q-warning)]" />
        <span className="font-medium">{item.reason}</span>
      </div>
      {query && (
        <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/50 p-2 font-mono text-xs">
          {query}
        </pre>
      )}
      {item.decision ? (
        <div className="mt-2">
          <Badge variant={item.decision === 'approved' ? 'default' : 'secondary'}>
            {item.decision === 'approved'
              ? t('agentChat.permission.approved')
              : t('agentChat.permission.denied')}
          </Badge>
        </div>
      ) : (
        <div className="mt-3 flex flex-wrap gap-2">
          <Button size="sm" onClick={() => onRespond(item.permissionId, true, false)}>
            {t('agentChat.permission.allow')}
          </Button>
          {item.canRemember && (
            <Button
              size="sm"
              variant="outline"
              onClick={() => onRespond(item.permissionId, true, true)}
            >
              {t('agentChat.permission.allowAlways')}
            </Button>
          )}
          <Button
            size="sm"
            variant="ghost"
            onClick={() => onRespond(item.permissionId, false, false)}
          >
            {t('agentChat.permission.deny')}
          </Button>
        </div>
      )}
    </div>
  );
}
