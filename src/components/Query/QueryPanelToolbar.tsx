import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Play, Square, AlertCircle, History, Shield, Lock, Plus, BookmarkPlus, Folder } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Tooltip } from '@/components/ui/tooltip';
import { Environment } from '../../lib/tauri';
import { ENVIRONMENT_CONFIG } from '../../lib/environment';
import { MONGO_TEMPLATES } from '../Editor/MongoEditor';

type EnvConfig = (typeof ENVIRONMENT_CONFIG)[keyof typeof ENVIRONMENT_CONFIG];

interface QueryPanelToolbarProps {
  loading: boolean;
  cancelling: boolean;
  sessionId: string | null;
  environment: Environment;
  envConfig: EnvConfig;
  readOnly: boolean;
  isMongo: boolean;
  keepResults: boolean;
  isExplainSupported: boolean;
  onExecute: () => void;
  onCancel: () => void;
  onExplain: () => void;
  onToggleKeepResults: () => void;
  onNewDocument: () => void;
  onHistoryOpen: () => void;
  onLibraryOpen: () => void;
  onSaveToLibrary: () => void;
  onTemplateSelect: (templateKey: keyof typeof MONGO_TEMPLATES) => void;
}

export function QueryPanelToolbar({
  loading,
  cancelling,
  sessionId,
  environment,
  envConfig,
  readOnly,
  isMongo,
  keepResults,
  isExplainSupported,
  onExecute,
  onCancel,
  onExplain,
  onToggleKeepResults,
  onNewDocument,
  onHistoryOpen,
  onLibraryOpen,
  onSaveToLibrary,
  onTemplateSelect,
}: QueryPanelToolbarProps) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center gap-2 p-2 border-b border-border bg-muted/20">
      <Button onClick={onExecute} disabled={loading || !sessionId} className="w-24 gap-2">
        {loading ? (
          <span className="flex items-center gap-2">{t('query.running')}</span>
        ) : (
          <>
            <Play size={16} className="fill-current" /> {t('query.run')}
          </>
        )}
      </Button>

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

      {loading && (
        <Button variant="destructive" onClick={onCancel} disabled={cancelling} className="w-24 gap-2">
          <Square size={16} className="fill-current" /> {t('query.stop')}
        </Button>
      )}

      {isMongo && sessionId && (
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

      {isMongo && (
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

      {!isMongo && (
        <>
          {isExplainSupported && (
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
        </>
      )}

      <div className="flex-1" />

      {!isMongo && (
        <button
          type="button"
          onClick={onToggleKeepResults}
          className={cn(
            'h-9 px-3 rounded-md border border-transparent text-xs font-medium transition-colors',
            keepResults
              ? 'bg-accent/10 text-accent border-accent/30'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted/50'
          )}
          aria-pressed={keepResults}
        >
          {t('query.keepResults')}
        </button>
      )}

      <Button
        variant="ghost"
        size="sm"
        onClick={onHistoryOpen}
        className="h-9 px-2 text-muted-foreground hover:text-foreground"
        title={t('query.history')}
      >
        <History size={16} className="mr-1" />
        {t('query.history')}
      </Button>

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
        {t('query.runHint')}
      </span>

      {!sessionId && (
        <span className="flex items-center gap-1.5 text-xs text-warning bg-warning/10 px-2 py-1 rounded-full border border-warning/20">
          <AlertCircle size={12} /> {t('query.noConnection')}
        </span>
      )}
    </div>
  );
}
