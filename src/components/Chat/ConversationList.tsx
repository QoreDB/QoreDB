// SPDX-License-Identifier: BUSL-1.1

import { MessageSquarePlus, Pencil, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { ConversationMeta } from '@/lib/agent';
import { cn } from '@/lib/utils';

interface ConversationListProps {
  conversations: ConversationMeta[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onRename: (id: string, title: string) => void;
  onDelete: (id: string) => void;
}

export function ConversationList({
  conversations,
  activeId,
  onSelect,
  onNew,
  onRename,
  onDelete,
}: ConversationListProps) {
  const { t } = useTranslation();
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftTitle, setDraftTitle] = useState('');

  const commitRename = (id: string) => {
    const title = draftTitle.trim();
    setEditingId(null);
    if (title) onRename(id, title);
  };

  return (
    <div className="flex w-60 shrink-0 flex-col border-r border-border">
      <div className="flex items-center justify-between px-3 py-2">
        <span className="text-xs font-medium uppercase text-muted-foreground">
          {t('agentChat.conversations')}
        </span>
        <Button
          size="icon"
          variant="ghost"
          className="h-7 w-7"
          onClick={onNew}
          title={t('agentChat.newConversation')}
        >
          <MessageSquarePlus size={15} />
        </Button>
      </div>
      <div className="flex-1 overflow-y-auto px-2 pb-2">
        {conversations.length === 0 && (
          <p className="px-1 py-2 text-xs text-muted-foreground">
            {t('agentChat.noConversations')}
          </p>
        )}
        {conversations.map(conversation => (
          <div
            key={conversation.id}
            className={cn(
              'group mb-1 rounded-md px-2 py-1.5 text-sm',
              conversation.id === activeId ? 'bg-accent/15' : 'hover:bg-muted/60'
            )}
          >
            {editingId === conversation.id ? (
              <Input
                autoFocus
                value={draftTitle}
                onChange={e => setDraftTitle(e.target.value)}
                onBlur={() => commitRename(conversation.id)}
                onKeyDown={e => {
                  if (e.key === 'Enter') commitRename(conversation.id);
                  if (e.key === 'Escape') setEditingId(null);
                }}
                className="h-7 text-sm"
              />
            ) : (
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() => onSelect(conversation.id)}
                  className="min-w-0 flex-1 truncate text-left"
                  title={conversation.title}
                >
                  {conversation.title || t('agentChat.untitled')}
                </button>
                <button
                  type="button"
                  className="hidden shrink-0 text-muted-foreground hover:text-foreground group-hover:block"
                  onClick={() => {
                    setEditingId(conversation.id);
                    setDraftTitle(conversation.title);
                  }}
                  title={t('agentChat.rename')}
                >
                  <Pencil size={13} />
                </button>
                <button
                  type="button"
                  className="hidden shrink-0 text-muted-foreground hover:text-destructive group-hover:block"
                  onClick={() => onDelete(conversation.id)}
                  title={t('agentChat.delete')}
                >
                  <Trash2 size={13} />
                </button>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
