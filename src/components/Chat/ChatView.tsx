// SPDX-License-Identifier: BUSL-1.1

import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { PanelLeftOpen } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AiProviderSelector } from '@/components/AI/AiProviderSelector';
import { QoreAiMark } from '@/components/Brand/QoreAiMark';
import { LicenseGate } from '@/components/License/LicenseGate';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useAgentChat } from '@/hooks/useAgentChat';
import {
  type ConversationMeta,
  chatDeleteConversation,
  chatGenerateTitle,
  chatListConversations,
  chatLoadConversation,
  chatRenameConversation,
  chatSaveConversation,
} from '@/lib/agent';
import { AI_PROVIDERS, type AiModelInfo, type AiProvider, aiListModels } from '@/lib/ai';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';
import { AgentPromptInput } from './AgentPromptInput';
import { AgentThread, usageTotalTokens } from './AgentThread';
import { ConversationList } from './ConversationList';

interface ChatViewProps {
  sessionId: string | null;
  connectionId?: string;
  connectionName?: string;
  environment?: string;
}

const CONVERSATION_SIDEBAR_STORAGE_KEY = 'qoredb_chat_conversation_sidebar';
const CONVERSATION_SIDEBAR_WIDTH = 256;

function loadConversationSidebarPreference(): boolean {
  try {
    return localStorage.getItem(CONVERSATION_SIDEBAR_STORAGE_KEY) !== 'collapsed';
  } catch {
    return true;
  }
}

export function ChatView({ sessionId, connectionId, connectionName, environment }: ChatViewProps) {
  const { t, i18n } = useTranslation();
  const reduceMotion = useReducedMotion();
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
  const [conversationSidebarVisible, setConversationSidebarVisible] = useState(
    loadConversationSidebarPreference
  );

  const setConversationSidebar = useCallback((visible: boolean) => {
    setConversationSidebarVisible(visible);
    try {
      localStorage.setItem(CONVERSATION_SIDEBAR_STORAGE_KEY, visible ? 'expanded' : 'collapsed');
    } catch {}
  }, []);

  const activeIdRef = useRef<string | null>(null);
  activeIdRef.current = activeId;
  const titleRef = useRef('');
  titleRef.current = title;

  const refreshList = useCallback(async () => {
    try {
      setConversations(await chatListConversations());
    } catch {}
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
      .catch(() => {});
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
        } catch {}
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

  const conversationTokens = useMemo(
    () =>
      chat.items.reduce(
        (sum, item) =>
          item.kind === 'assistant' && item.usage ? sum + usageTotalTokens(item.usage) : sum,
        0
      ),
    [chat.items]
  );

  return (
    <LicenseGate feature="ai">
      <div className="flex h-full min-h-0 min-w-0 flex-1 overflow-hidden">
        <AnimatePresence initial={false}>
          {conversationSidebarVisible && (
            <motion.div
              key="conversation-sidebar"
              initial={{ width: 0, opacity: 0, x: -12 }}
              animate={{ width: CONVERSATION_SIDEBAR_WIDTH, opacity: 1, x: 0 }}
              exit={{ width: 0, opacity: 0, x: -12 }}
              transition={
                reduceMotion ? { duration: 0 } : { duration: 0.2, ease: [0.22, 1, 0.36, 1] }
              }
              className="flex min-h-0 shrink-0 overflow-hidden"
            >
              <ConversationList
                conversations={conversations}
                activeId={activeId}
                onSelect={id => void handleSelect(id)}
                onNew={handleNew}
                onRename={(id, nextTitle) => void handleRename(id, nextTitle)}
                onDelete={id => void handleDelete(id)}
                onCollapse={() => setConversationSidebar(false)}
              />
            </motion.div>
          )}
        </AnimatePresence>
        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
          <div className="flex min-h-12 items-center gap-2 border-b border-border px-3 py-2">
            {!conversationSidebarVisible && (
              <Button
                size="icon"
                variant="ghost"
                className="h-8 w-8 shrink-0"
                onClick={() => setConversationSidebar(true)}
                title={t('agentChat.showConversations')}
                aria-label={t('agentChat.showConversations')}
              >
                <PanelLeftOpen size={16} />
              </Button>
            )}
            <div className="flex min-w-0 flex-1 items-center gap-2">
              <QoreAiMark compact size={20} />
              <span className="truncate text-sm font-medium">{title || t('agentChat.title')}</span>
              {conversationTokens > 0 && (
                <span
                  className="shrink-0 select-none text-[11px] text-muted-foreground/70"
                  title={t('agentChat.usage.conversationTotal')}
                >
                  {t('agentChat.usage.tokens', {
                    value: new Intl.NumberFormat(i18n.language, {
                      notation: 'compact',
                      maximumFractionDigits: 1,
                    }).format(conversationTokens),
                  })}
                </span>
              )}
            </div>
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
          <div className="min-h-0 flex-1 overflow-y-auto scroll-smooth">
            <div className="mx-auto min-h-full w-full max-w-4xl px-5 py-6">
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
            <p className="mx-auto w-full max-w-4xl px-5 pb-1 text-xs text-muted-foreground">
              {t('agentChat.noConnection')}
            </p>
          )}
          {sessionId && !isReady && (
            <p className="mx-auto w-full max-w-4xl px-5 pb-1 text-xs text-muted-foreground">
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
