// SPDX-License-Identifier: BUSL-1.1

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AiProviderSelector } from '@/components/AI/AiProviderSelector';
import { LicenseGate } from '@/components/License/LicenseGate';
import { Badge } from '@/components/ui/badge';
import { useAgentChat } from '@/hooks/useAgentChat';
import {
  chatDeleteConversation,
  chatGenerateTitle,
  chatListConversations,
  chatLoadConversation,
  chatRenameConversation,
  chatSaveConversation,
  type ConversationMeta,
} from '@/lib/agent';
import { AI_PROVIDERS, type AiModelInfo, type AiProvider, aiListModels } from '@/lib/ai';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';
import { AgentPromptInput } from './AgentPromptInput';
import { AgentThread } from './AgentThread';
import { ConversationList } from './ConversationList';

interface ChatViewProps {
  sessionId: string | null;
  connectionId?: string;
  connectionName?: string;
  environment?: string;
}

export function ChatView({ sessionId, connectionId, connectionName, environment }: ChatViewProps) {
  const { t } = useTranslation();
  const {
    preferredProvider,
    setPreferredProvider,
    preferredModels,
    setPreferredModel,
    providerStatuses,
    isReady,
    getConfig,
  } = useAiPreferences();

  const [models, setModels] = useState<AiModelInfo[]>([]);

  const [conversations, setConversations] = useState<ConversationMeta[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [title, setTitle] = useState('');

  const activeIdRef = useRef<string | null>(null);
  activeIdRef.current = activeId;
  const titleRef = useRef('');
  titleRef.current = title;

  const refreshList = useCallback(async () => {
    try {
      setConversations(await chatListConversations());
    } catch {
      // Core build or unreadable directory: leave the list empty.
    }
  }, []);

  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  useEffect(() => {
    let cancelled = false;
    const fallback = AI_PROVIDERS.find(p => p.id === preferredProvider)?.models ?? [];
    setModels(fallback);
    aiListModels(preferredProvider)
      .then(list => {
        if (!cancelled && list.length > 0) setModels(list);
      })
      .catch(() => {
        // Core build or provider unreachable: keep the curated fallback.
      });
    return () => {
      cancelled = true;
    };
  }, [preferredProvider]);

  const selectedModel =
    preferredModels[preferredProvider] ??
    providerStatuses.find(s => s.provider === preferredProvider)?.default_model ??
    models[0]?.id ??
    '';

  const chat = useAgentChat({
    sessionId,
    connectionId,
    onDone: () => void persist(),
  });
  const { toStoredMessages } = chat;

  const persist = useCallback(async () => {
    const messages = toStoredMessages();
    if (messages.length === 0) return;
    const id = activeIdRef.current ?? crypto.randomUUID();
    let nextTitle = titleRef.current;
    if (!nextTitle) {
      const firstUser = messages.find(m => m.role === 'user');
      nextTitle = (firstUser?.content ?? '').slice(0, 60);
      if (firstUser) {
        try {
          nextTitle = await chatGenerateTitle(firstUser.content, getConfig());
        } catch {
          // Keep the truncated prompt as fallback title.
        }
      }
    }
    try {
      const now = new Date().toISOString();
      const saved = await chatSaveConversation({
        id,
        title: nextTitle,
        created_at: now,
        updated_at: now,
        messages,
        scope: connectionName ? [connectionName] : [],
      });
      setActiveId(saved.id);
      setTitle(saved.title);
      await refreshList();
    } catch (err) {
      console.error('Failed to save conversation', err);
    }
  }, [toStoredMessages, getConfig, connectionName, refreshList]);

  const handleNew = useCallback(() => {
    chat.reset();
    setActiveId(null);
    setTitle('');
  }, [chat]);

  const handleSelect = useCallback(
    async (id: string) => {
      if (chat.loading) return;
      try {
        const conversation = await chatLoadConversation(id);
        chat.loadMessages(conversation.messages);
        setActiveId(conversation.id);
        setTitle(conversation.title);
      } catch (err) {
        console.error('Failed to load conversation', err);
      }
    },
    [chat]
  );

  const handleRename = useCallback(
    async (id: string, nextTitle: string) => {
      try {
        await chatRenameConversation(id, nextTitle);
        if (activeIdRef.current === id) setTitle(nextTitle);
        await refreshList();
      } catch (err) {
        console.error('Failed to rename conversation', err);
      }
    },
    [refreshList]
  );

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await chatDeleteConversation(id);
        if (activeIdRef.current === id) handleNew();
        await refreshList();
      } catch (err) {
        console.error('Failed to delete conversation', err);
      }
    },
    [refreshList, handleNew]
  );

  const providerHasKey = useMemo(() => {
    const record = {} as Record<AiProvider, boolean>;
    for (const p of AI_PROVIDERS) {
      record[p.id] = p.requiresKey
        ? (providerStatuses.find(s => s.provider === p.id)?.has_key ?? false)
        : true;
    }
    return record;
  }, [providerStatuses]);

  const disabled = !sessionId || !isReady;

  return (
    <LicenseGate feature="ai">
      <div className="flex h-full min-h-0">
        <ConversationList
          conversations={conversations}
          activeId={activeId}
          onSelect={id => void handleSelect(id)}
          onNew={handleNew}
          onRename={(id, nextTitle) => void handleRename(id, nextTitle)}
          onDelete={id => void handleDelete(id)}
        />
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex items-center gap-2 border-b border-border px-4 py-2">
            <span className="min-w-0 flex-1 truncate text-sm font-medium">
              {title || t('agentChat.title')}
            </span>
            {connectionName && (
              <Badge variant="outline" className="shrink-0 gap-1 font-normal">
                {connectionName}
                {environment && environment !== 'development' && (
                  <span
                    className={
                      environment === 'production'
                        ? 'text-[var(--q-env-prod)]'
                        : 'text-[var(--q-env-staging)]'
                    }
                  >
                    · {environment}
                  </span>
                )}
              </Badge>
            )}
            <div className="w-40 shrink-0">
              <AiProviderSelector
                provider={preferredProvider}
                onProviderChange={setPreferredProvider}
                providerHasKey={providerHasKey}
              />
            </div>
            <Select
              value={selectedModel}
              onValueChange={model => setPreferredModel(preferredProvider, model)}
            >
              <SelectTrigger className="h-8 w-48 shrink-0 text-xs">
                <SelectValue placeholder={t('agentChat.model')} />
              </SelectTrigger>
              <SelectContent>
                {models.map(model => (
                  <SelectItem key={model.id} value={model.id}>
                    {model.label}
                  </SelectItem>
                ))}
                {selectedModel && !models.some(m => m.id === selectedModel) && (
                  <SelectItem value={selectedModel}>{selectedModel}</SelectItem>
                )}
              </SelectContent>
            </Select>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            <div className="mx-auto min-h-full w-full max-w-3xl px-4 py-6">
              <AgentThread
                items={chat.items}
                loading={chat.loading}
                onRespondPermission={(permissionId, approved, remember) =>
                  void chat.respondPermission(permissionId, approved, remember)
                }
              />
            </div>
          </div>
          {!sessionId && (
            <p className="mx-auto w-full max-w-3xl px-4 pb-1 text-xs text-muted-foreground">
              {t('agentChat.noConnection')}
            </p>
          )}
          {sessionId && !isReady && (
            <p className="mx-auto w-full max-w-3xl px-4 pb-1 text-xs text-muted-foreground">
              {t('agentChat.noApiKey')}
            </p>
          )}
          <AgentPromptInput
            onSubmit={prompt => void chat.sendMessage(prompt, getConfig())}
            onCancel={() => void chat.cancel()}
            loading={chat.loading}
            disabled={disabled}
          />
        </div>
      </div>
    </LicenseGate>
  );
}
