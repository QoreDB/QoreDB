// SPDX-License-Identifier: Apache-2.0

import {
  AlertCircle,
  BookmarkPlus,
  Check,
  Database,
  Folder,
  History,
  Layers,
  Lock,
  MoreHorizontal,
  Network,
  Play,
  Plus,
  RotateCcw,
  Shield,
  Sparkles,
  Square,
} from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { HelpIcon } from '@/components/ui/help-icon';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tooltip } from '@/components/ui/tooltip';
import { createFederationTab } from '@/lib/tabs';
import { useTabContext } from '@/providers/TabProvider';
import { getModifierKey } from '@/utils/platform';
import type { ENVIRONMENT_CONFIG } from '../../lib/environment';
import type { Environment, Namespace } from '../../lib/tauri';
import type { MONGO_TEMPLATES } from '../Editor/mongo-constants';

type EnvConfig = (typeof ENVIRONMENT_CONFIG)[keyof typeof ENVIRONMENT_CONFIG];

interface QueryPanelToolbarProps {
  loading: boolean;
  cancelling: boolean;
  sessionId: string | null;
  environment: Environment;
  envConfig: EnvConfig;
  readOnly: boolean;
  isDocumentBased: boolean;
  keepResults: boolean;
  isExplainSupported: boolean;
  canCancel: boolean;
  connectionName?: string;
  connectionDatabase?: string;
  activeNamespace?: Namespace | null;
  onExecute: () => void;
  onCancel: () => void;
  onExplain: () => void;
  onToggleKeepResults: () => void;
  onNewDocument: () => void;
  onHistoryOpen: () => void;
  onLibraryOpen: () => void;
  onSaveToLibrary: () => void;
  onTemplateSelect: (templateKey: keyof typeof MONGO_TEMPLATES) => void;
  onAiToggle?: () => void;
  aiPanelOpen?: boolean;
  supportsTransactions?: boolean;
  transactionActive?: boolean;
  transactionStatements?: number;
  onBeginTransaction?: () => void;
  onCommitTransaction?: () => void;
  onRollbackTransaction?: () => void;
}

export function QueryPanelToolbar({
  loading,
  cancelling,
  sessionId,
  environment,
  envConfig,
  readOnly,
  isDocumentBased,
  keepResults,
  isExplainSupported,
  canCancel,
  connectionName,
  connectionDatabase,
  activeNamespace,
  onExecute,
  onCancel,
  onExplain,
  onToggleKeepResults,
  onNewDocument,
  onHistoryOpen,
  onLibraryOpen,
  onSaveToLibrary,
  onTemplateSelect,
  onAiToggle,
  aiPanelOpen,
  supportsTransactions,
  transactionActive,
  transactionStatements = 0,
  onBeginTransaction,
  onCommitTransaction,
  onRollbackTransaction,
}: QueryPanelToolbarProps) {
  const { t } = useTranslation();
  const { openTab } = useTabContext();
  const [templateSelectValue, setTemplateSelectValue] = useState<string | undefined>(undefined);

  // Priority: activeNamespace.database > connectionDatabase
  const displayDatabase = activeNamespace?.database || connectionDatabase;
  const displaySchema = activeNamespace?.schema;

  return (
    <div className="flex items-center gap-2 p-2 border-b border-border bg-muted/20">
      {/* --- PRIMARY ZONE --- */}

      <Button
        data-tour="query-execute"
        onClick={onExecute}
        disabled={loading || !sessionId}
        className="gap-2"
      >
        {loading ? (
          <span className="flex items-center gap-2">{t('query.running')}</span>
        ) : (
          <>
            <Play size={16} className="fill-current" /> {t('query.run')}
          </>
        )}
      </Button>

      {/* Database context badge */}
      {sessionId && (connectionName || displayDatabase) && (
        <span className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium rounded-md bg-muted/50 text-muted-foreground border border-border">
          <Database size={12} className="text-accent" />
          {connectionName && <span className="truncate max-w-32">{connectionName}</span>}
          {displayDatabase && (
            <>
              {connectionName && <span className="text-muted-foreground/50">→</span>}
              <span className="truncate max-w-24 font-mono">{displayDatabase}</span>
              {displaySchema && (
                <>
                  <span className="text-muted-foreground/40">.</span>
                  <span className="truncate max-w-20 font-mono">{displaySchema}</span>
                </>
              )}
            </>
          )}
        </span>
      )}

      {sessionId && environment !== 'development' && (
        <span
          className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-bold rounded-full border"
          style={{
            backgroundColor: envConfig.bgSoft,
            color: envConfig.color,
            borderColor: envConfig.color,
          }}
        >
          <Shield size={12} />
          {envConfig.labelShort}
        </span>
      )}

      {sessionId && readOnly && (
        <span className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-bold rounded-full border border-warning/30 bg-warning/10 text-warning">
          <Lock size={12} />
          {t('environment.readOnly')}
        </span>
      )}

      {loading &&
        (canCancel ? (
          <Button
            variant="destructive"
            onClick={onCancel}
            disabled={cancelling}
            className="w-24 gap-2"
          >
            <Square size={16} className="fill-current" /> {t('query.stop')}
          </Button>
        ) : (
          <Tooltip content={t('query.cancelNotSupported')}>
            <span>
              <Button variant="destructive" disabled className="w-24 gap-2">
                <Square size={16} className="fill-current" /> {t('query.stop')}
              </Button>
            </span>
          </Tooltip>
        ))}

      {isDocumentBased && sessionId && (
        <Button
          variant="ghost"
          size="sm"
          className="h-9 px-2 text-muted-foreground hover:text-foreground ml-2"
          onClick={onNewDocument}
          title={t('document.new')}
        >
          <Plus size={16} className="mr-1" />
          <span className="hidden sm:inline">{t('document.new')}</span>
        </Button>
      )}

      {isDocumentBased && (
        <Select
          value={templateSelectValue}
          onValueChange={value => {
            onTemplateSelect(value as keyof typeof MONGO_TEMPLATES);
            setTemplateSelectValue(undefined);
          }}
        >
          <SelectTrigger className="h-9 w-37.5">
            <SelectValue placeholder="Templates..." />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="find">find()</SelectItem>
            <SelectItem value="findOne">findOne()</SelectItem>
            <SelectItem value="aggregate">aggregate()</SelectItem>
            <SelectItem value="insertOne">insertOne()</SelectItem>
            <SelectItem value="updateOne">updateOne()</SelectItem>
            <SelectItem value="deleteOne">deleteOne()</SelectItem>
          </SelectContent>
        </Select>
      )}

      {/* Transaction controls — contextual, only when active/supported */}
      {supportsTransactions && sessionId && !isDocumentBased && (
        <>
          <div className="h-5 w-px bg-border/50" />
          {!transactionActive ? (
            <div className="flex items-center gap-1">
              <Tooltip content={t('transaction.tooltipBegin')}>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={onBeginTransaction}
                  disabled={loading}
                  className="h-9 px-2 text-muted-foreground hover:text-foreground"
                >
                  {t('transaction.begin')}
                </Button>
              </Tooltip>
              <HelpIcon content={t('help.transactions')} />
            </div>
          ) : (
            <div className="flex items-center gap-1">
              <span className="flex items-center gap-1.5 px-2 py-1 text-xs font-bold rounded-full border border-accent/30 bg-accent/10 text-accent">
                <span className="w-1.5 h-1.5 rounded-full bg-accent animate-pulse" />
                {t('transaction.active')}
                {transactionStatements > 0 && (
                  <span className="text-accent/70 font-normal">({transactionStatements})</span>
                )}
              </span>
              <Tooltip content={t('transaction.tooltipCommit')}>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={onCommitTransaction}
                  disabled={loading}
                  className="h-9 px-2 text-success hover:text-success hover:bg-success/10"
                >
                  <Check size={16} className="mr-1" />
                  {t('transaction.commit')}
                </Button>
              </Tooltip>
              <Tooltip content={t('transaction.tooltipRollback')}>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={onRollbackTransaction}
                  disabled={loading}
                  className="h-9 px-2 text-destructive hover:text-destructive hover:bg-destructive/10"
                >
                  <RotateCcw size={14} className="mr-1" />
                  {t('transaction.rollback')}
                </Button>
              </Tooltip>
            </div>
          )}
        </>
      )}

      <div className="flex-1" />

      {/* --- SECONDARY ZONE (overflow menu) --- */}

      <span className="text-xs text-muted-foreground hidden sm:inline-block">
        {t('query.runHint', { modifier: getModifierKey() })}
      </span>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-9 w-9 text-muted-foreground hover:text-foreground"
            aria-label={t('toolbar.moreActions')}
          >
            <MoreHorizontal size={16} />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-52">
          {/* Query Tools */}
          <DropdownMenuLabel>{t('toolbar.queryTools')}</DropdownMenuLabel>
          <DropdownMenuGroup>
            {!isDocumentBased && isExplainSupported && (
              <DropdownMenuItem onClick={onExplain} disabled={!sessionId || loading}>
                {t('query.explain')}
              </DropdownMenuItem>
            )}
            {!isDocumentBased && (
              <DropdownMenuCheckboxItem
                checked={keepResults}
                onCheckedChange={() => onToggleKeepResults()}
              >
                <Layers size={14} className="mr-2" />
                {t('query.keepResults')}
              </DropdownMenuCheckboxItem>
            )}
          </DropdownMenuGroup>

          <DropdownMenuSeparator />

          {/* Library */}
          <DropdownMenuLabel>{t('toolbar.library')}</DropdownMenuLabel>
          <DropdownMenuGroup>
            <DropdownMenuItem onClick={onSaveToLibrary}>
              <BookmarkPlus size={14} className="mr-2" />
              {t('library.save')}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={onLibraryOpen}>
              <Folder size={14} className="mr-2" />
              {t('library.open')}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={onHistoryOpen}>
              <History size={14} className="mr-2" />
              {t('query.history')}
            </DropdownMenuItem>
          </DropdownMenuGroup>

          <DropdownMenuSeparator />

          {/* Integrations */}
          <DropdownMenuLabel>{t('toolbar.integrations')}</DropdownMenuLabel>
          <DropdownMenuGroup>
            {onAiToggle && (
              <DropdownMenuCheckboxItem checked={aiPanelOpen} onCheckedChange={() => onAiToggle()}>
                <Sparkles size={14} className="mr-2" />
                {t('ai.title')}
              </DropdownMenuCheckboxItem>
            )}
            {!isDocumentBased && (
              <DropdownMenuItem onClick={() => openTab(createFederationTab())}>
                <Network size={14} className="mr-2" />
                {t('federation.badge')}
              </DropdownMenuItem>
            )}
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>

      {!sessionId && (
        <span className="flex items-center gap-1.5 text-xs text-warning bg-warning/10 px-2 py-1 rounded-full border border-warning/20">
          <AlertCircle size={12} /> {t('query.noConnection')}
        </span>
      )}
    </div>
  );
}
