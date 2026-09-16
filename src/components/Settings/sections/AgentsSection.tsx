// SPDX-License-Identifier: Apache-2.0

import { Bot, Check, ChevronRight, Copy } from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { getDriverMetadata } from '@/lib/connection/drivers';
import {
  getMcpBinaryStatus,
  getSafetyPolicy,
  type McpBinaryStatus,
  type SafetyPolicy,
  type SavedConnection,
  setConnectionExposed,
} from '@/lib/tauri';
import { useSessionContext } from '@/providers/SessionProvider';
import { useWorkspace } from '@/providers/WorkspaceProvider';
import { SettingsCard } from '../SettingsCard';
import { buildMcpSnippets, type McpClientSnippet } from './agentsSnippets';

const HEADLESS_QUERY_TIMEOUT_MS = 30_000;

interface AgentsSectionProps {
  searchQuery?: string;
}

export function AgentsSection({ searchQuery }: AgentsSectionProps) {
  const { t } = useTranslation();
  const { savedConnections, refreshSidebar } = useSessionContext();
  const { projectId, activeWorkspace } = useWorkspace();
  const [status, setStatus] = useState<McpBinaryStatus | null>(null);
  const [policy, setPolicy] = useState<SafetyPolicy | null>(null);

  useEffect(() => {
    let active = true;
    getMcpBinaryStatus()
      .then(result => active && setStatus(result))
      .catch(() => active && setStatus({ path: null, version: null }));
    getSafetyPolicy()
      .then(result => active && result.success && result.policy && setPolicy(result.policy))
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);

  const workspacePath =
    activeWorkspace && activeWorkspace.source !== 'default' ? activeWorkspace.path : undefined;
  const snippets = buildMcpSnippets(status?.path ?? 'qore-mcp', workspacePath);

  return (
    <>
      <SettingsCard
        id="agents-connections"
        title={t('settings.agents.connections.title')}
        description={t('settings.agents.connections.description')}
        searchQuery={searchQuery}
      >
        <ConnectionExposure
          connections={savedConnections}
          projectId={projectId}
          onChanged={refreshSidebar}
        />
      </SettingsCard>

      <SettingsCard
        id="agents-status"
        title={t('settings.agents.status.title')}
        searchQuery={searchQuery}
      >
        <BinaryStatus status={status} />
        <Disclosure title={t('settings.agents.config.title')}>
          <p className="mb-3 text-xs text-muted-foreground">
            {t('settings.agents.config.description')}
          </p>
          <p className="mb-3 text-xs text-muted-foreground">
            {workspacePath
              ? t('settings.agents.connections.storeWorkspace', { path: workspacePath })
              : t('settings.agents.connections.storeDefault')}
          </p>
          <div className="space-y-3">
            {snippets.map(snippet => (
              <SnippetBlock key={snippet.id} snippet={snippet} />
            ))}
          </div>
        </Disclosure>
      </SettingsCard>

      <Disclosure id="agents-limits" title={t('settings.agents.limits.title')} className="py-4">
        <p className="mb-3 text-xs text-muted-foreground">
          {t('settings.agents.limits.description')}
        </p>
        <AppliedLimits policy={policy} />
      </Disclosure>
    </>
  );
}

function Disclosure({
  id,
  title,
  className,
  children,
}: {
  id?: string;
  title: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <details id={id} className={`group ${className ?? 'mt-3 border-t border-border/50 pt-3'}`}>
      <summary className="flex cursor-pointer list-none items-center justify-between gap-3 text-sm font-medium text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring">
        {title}
        <ChevronRight
          size={14}
          className="shrink-0 text-muted-foreground transition-transform group-open:rotate-90"
        />
      </summary>
      <div className="mt-3">{children}</div>
    </details>
  );
}

function BinaryStatus({ status }: { status: McpBinaryStatus | null }) {
  const { t } = useTranslation();
  if (!status) {
    return <p className="text-xs text-muted-foreground">{t('settings.agents.status.loading')}</p>;
  }
  if (!status.path) {
    return (
      <div className="space-y-2">
        <Badge variant="outline">{t('settings.agents.status.notFound')}</Badge>
        <p className="text-xs text-muted-foreground">{t('settings.agents.status.hint')}</p>
      </div>
    );
  }
  return (
    <div className="space-y-2 text-sm">
      <div className="flex items-center gap-2">
        <Badge>{t('settings.agents.status.found')}</Badge>
        {status.version && (
          <span className="text-xs text-muted-foreground">
            {t('settings.agents.status.version')} {status.version}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        <span className="text-xs text-muted-foreground">{t('settings.agents.status.path')}</span>
        <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs break-all">
          {status.path}
        </code>
      </div>
    </div>
  );
}

function SnippetBlock({ snippet }: { snippet: McpClientSnippet }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(snippet.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      toast.error(t('settings.agents.config.copyFailed'));
    }
  }

  return (
    <div className="rounded-md border border-border">
      <div className="flex items-center justify-between border-b border-border px-3 py-1.5">
        <span className="text-xs font-medium">{t(snippet.labelKey)}</span>
        <Button variant="ghost" size="sm" className="h-7 gap-1.5 px-2" onClick={copy}>
          {copied ? <Check size={12} /> : <Copy size={12} />}
          {copied ? t('settings.agents.config.copied') : t('settings.agents.config.copy')}
        </Button>
      </div>
      <pre className="overflow-x-auto px-3 py-2 font-mono text-xs">{snippet.content}</pre>
    </div>
  );
}

function ConnectionExposure({
  connections,
  projectId,
  onChanged,
}: {
  connections: SavedConnection[];
  projectId: string;
  onChanged: () => void;
}) {
  const { t } = useTranslation();
  const [pending, setPending] = useState<string | null>(null);

  async function toggle(connection: SavedConnection, exposed: boolean) {
    setPending(connection.id);
    try {
      const result = await setConnectionExposed(projectId, connection.id, exposed);
      if (!result.success) throw new Error(result.error);
      onChanged();
    } catch (error) {
      toast.error(error instanceof Error && error.message ? error.message : String(error));
    } finally {
      setPending(null);
    }
  }

  if (connections.length === 0) {
    return (
      <div className="flex items-start gap-2 rounded-md border border-dashed border-border px-3 py-2 text-xs text-muted-foreground">
        <Bot size={14} className="mt-0.5 shrink-0" />
        {t('settings.agents.connections.empty')}
      </div>
    );
  }

  return (
    <ul className="divide-y divide-border rounded-md border border-border">
      {connections.map(connection => (
        <li key={connection.id} className="flex items-center justify-between gap-3 px-3 py-2">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-sm">
              <span className="truncate font-medium">{connection.name}</span>
              <Badge variant="outline">{t(`environment.${connection.environment}`)}</Badge>
            </div>
            <div className="truncate text-xs text-muted-foreground">
              {getDriverMetadata(connection.driver).label}
            </div>
          </div>
          <Switch
            aria-label={t('settings.agents.connections.toggle')}
            checked={connection.expose_to_agents ?? false}
            disabled={pending === connection.id}
            onCheckedChange={checked => toggle(connection, checked)}
          />
        </li>
      ))}
    </ul>
  );
}

function AppliedLimits({ policy }: { policy: SafetyPolicy | null }) {
  const { t } = useTranslation();
  const timeoutMs = Math.min(policy?.max_query_duration_ms ?? Infinity, HEADLESS_QUERY_TIMEOUT_MS);
  const maxRows = policy?.max_result_rows ?? null;
  const rateLimit = policy?.query_rate_limit_enabled ?? true;

  return (
    <ul className="list-disc space-y-1 pl-5 text-xs text-muted-foreground">
      <li>{t('settings.agents.limits.readOnly')}</li>
      <li>{t('settings.agents.limits.timeout', { seconds: Math.round(timeoutMs / 1000) })}</li>
      <li>
        {maxRows === null
          ? t('settings.agents.limits.rowsUnlimited')
          : t('settings.agents.limits.rows', { count: maxRows })}
      </li>
      <li>
        {t('settings.agents.limits.rateLimit', {
          state: rateLimit ? t('common.enabled') : t('common.disabled'),
        })}
      </li>
      <li>{t('settings.agents.limits.idle')}</li>
      <li>{t('settings.agents.limits.audit')}</li>
    </ul>
  );
}
