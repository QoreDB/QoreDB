// SPDX-License-Identifier: Apache-2.0

import {
  AlertCircle,
  BookmarkPlus,
  Database,
  Folder,
  History,
  Layers,
  Lock,
  Play,
  Plus,
  Shield,
  Sparkles,
  Square,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Tooltip } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
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
}: QueryPanelToolbarProps) {
  const { t } = useTranslation();

  // Priority: activeNamespace.database > connectionDatabase
  const displayDatabase = activeNamespace?.database || connectionDatabase;
  const displaySchema = activeNamespace?.schema;

  return (
    <div className="flex items-center gap-2 p-2 border-b border-border bg-muted/20">
      <Button onClick={onExecute} disabled={loading || !sessionId} className="gap-2">
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
        <select
          className="h-9 rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          onChange={e => {
            onTemplateSelect(e.target.value as keyof typeof MONGO_TEMPLATES);
            e.currentTarget.value = '';
          }}
          defaultValue=""
        >
          <option value="" disabled>
            Templates...
          </option>
          <option value="find">find()</option>
          <option value="findOne">findOne()</option>
          <option value="aggregate">aggregate()</option>
          <option value="insertOne">insertOne()</option>
          <option value="updateOne">updateOne()</option>
          <option value="deleteOne">deleteOne()</option>
        </select>
      )}

      {!isDocumentBased && isExplainSupported && (
        <Button
          variant="ghost"
          size="sm"
          onClick={onExplain}
          disabled={!sessionId || loading}
          className="h-9 px-2 text-muted-foreground hover:text-foreground"
        >
          {t('query.explain')}
        </Button>
      )}

      <div className="flex-1" />

      {!isDocumentBased && (
        <Tooltip content={t('query.keepResults')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={onToggleKeepResults}
            className={cn(
              'h-9 w-9',
              keepResults
                ? 'text-accent bg-accent/10 hover:bg-accent/20'
                : 'text-muted-foreground hover:text-foreground'
            )}
          >
            <Layers size={16} />
          </Button>
        </Tooltip>
      )}

      {onAiToggle && (
        <Tooltip content={t('ai.title')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={onAiToggle}
            className={cn(
              'h-9 w-9',
              aiPanelOpen
                ? 'text-accent bg-accent/10 hover:bg-accent/20'
                : 'text-muted-foreground hover:text-foreground'
            )}
          >
            <Sparkles size={16} />
          </Button>
        </Tooltip>
      )}

      <Tooltip content={t('query.history')}>
        <Button
          variant="ghost"
          size="icon"
          onClick={onHistoryOpen}
          className="h-9 w-9 text-muted-foreground hover:text-foreground"
        >
          <History size={16} />
        </Button>
      </Tooltip>

      <Tooltip content={t('library.save')}>
        <Button
          variant="ghost"
          size="icon"
          onClick={onSaveToLibrary}
          className="h-9 w-9 text-muted-foreground hover:text-foreground"
          aria-label={t('library.save')}
        >
          <BookmarkPlus size={16} />
        </Button>
      </Tooltip>

      <Tooltip content={t('library.open')}>
        <Button
          variant="ghost"
          size="icon"
          onClick={onLibraryOpen}
          className="h-9 w-9 text-muted-foreground hover:text-foreground"
          aria-label={t('library.open')}
        >
          <Folder size={16} />
        </Button>
      </Tooltip>

      <span className="text-xs text-muted-foreground hidden sm:inline-block">
        {t('query.runHint', { modifier: getModifierKey() })}
      </span>

      {!sessionId && (
        <span className="flex items-center gap-1.5 text-xs text-warning bg-warning/10 px-2 py-1 rounded-full border border-warning/20">
          <AlertCircle size={12} /> {t('query.noConnection')}
        </span>
      )}
    </div>
  );
}
