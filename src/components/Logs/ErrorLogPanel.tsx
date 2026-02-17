// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { getErrorLogs, clearErrorLogs, ErrorLogEntry } from '../../lib/errorLog';
import { exportLogs } from '../../lib/tauri';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import {
  Bug,
  Trash2,
  AlertCircle,
  AlertTriangle,
  Info,
  Search,
  RefreshCw,
  Download,
} from 'lucide-react';
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { toast } from 'sonner';

interface ErrorLogPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

type FilterLevel = 'all' | 'error' | 'warn' | 'info';

export function ErrorLogPanel({ isOpen, onClose }: ErrorLogPanelProps) {
  const { t } = useTranslation();
  const [logs, setLogs] = useState<ErrorLogEntry[]>([]);
  const [filter, setFilter] = useState<FilterLevel>('all');
  const [search, setSearch] = useState('');
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    if (isOpen) {
      loadLogs();
    }
  }, [isOpen]);

  function loadLogs() {
    setLogs(getErrorLogs());
  }

  function handleClear() {
    if (confirm(t('logs.clearConfirm'))) {
      clearErrorLogs();
      loadLogs();
    }
  }

  const filteredLogs = logs.filter(log => {
    if (filter !== 'all' && log.level !== filter) return false;
    if (search) {
      const lowerSearch = search.toLowerCase();
      return (
        log.message.toLowerCase().includes(lowerSearch) ||
        log.source.toLowerCase().includes(lowerSearch) ||
        log.details?.toLowerCase().includes(lowerSearch)
      );
    }
    return true;
  });

  function formatTime(timestamp: number): string {
    return new Date(timestamp).toLocaleString();
  }

  function getLevelIcon(level: string) {
    switch (level) {
      case 'error':
        return <AlertCircle size={14} className="text-error" />;
      case 'warn':
        return <AlertTriangle size={14} className="text-warning" />;
      case 'info':
        return <Info size={14} className="text-accent" />;
      default:
        return <Bug size={14} />;
    }
  }

  function handleOpenChange(open: boolean) {
    if (!open) {
      onClose();
    }
  }

  async function handleExport() {
    setExporting(true);
    try {
      const response = await exportLogs();
      if (!response.success || !response.content) {
        toast.error(t('logs.exportError'), {
          description: response.error || t('common.unknownError'),
        });
        return;
      }

      const filePath = await save({
        defaultPath: response.filename || 'qoredb-logs.log',
        filters: [{ name: 'Logs', extensions: ['log', 'txt'] }],
      });

      if (!filePath) return;

      await writeTextFile(filePath, response.content);
      const name = filePath.split(/[\\/]/).pop() || filePath;
      toast.success(t('logs.exportSuccess', { name }));
    } catch (err) {
      toast.error(t('logs.exportError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setExporting(false);
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-3xl max-h-[85vh] flex flex-col p-0 gap-0">
        <DialogHeader className="px-4 py-3 border-b border-border">
          <DialogTitle className="flex items-center gap-2 text-base">
            <Bug size={18} className="text-error" />
            {t('logs.title')}
            <span className="text-xs font-normal text-muted-foreground bg-muted px-1.5 py-0.5 rounded ml-2">
              {filteredLogs.length}
            </span>
          </DialogTitle>
        </DialogHeader>

        <div className="flex items-center gap-2 px-4 py-2 border-b border-border bg-muted/20">
          <div className="flex items-center gap-1">
            {(['all', 'error', 'warn', 'info'] as FilterLevel[]).map(level => (
              <button
                key={level}
                className={cn(
                  'px-2 py-1 text-xs font-medium rounded-md transition-colors',
                  filter === level
                    ? 'bg-accent text-accent-foreground'
                    : 'text-muted-foreground hover:text-foreground hover:bg-muted'
                )}
                onClick={() => setFilter(level)}
              >
                {t(`logs.filter.${level}`)}
              </button>
            ))}
          </div>

          <div className="flex-1" />

          <div className="relative">
            <Search
              size={14}
              className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
            />
            <input
              type="text"
              placeholder={t('logs.searchPlaceholder')}
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="h-8 pl-8 pr-3 text-sm rounded-md border border-input bg-background focus:outline-none focus:ring-1 focus:ring-ring w-48"
            />
          </div>

          <Button
            variant="ghost"
            size="icon"
            onClick={loadLogs}
            className="h-8 w-8"
            title={t('logs.refresh')}
          >
            <RefreshCw size={14} />
          </Button>

          <Button
            variant="ghost"
            size="sm"
            onClick={handleExport}
            className="h-8 text-xs text-muted-foreground hover:text-foreground"
            disabled={exporting}
          >
            <Download size={14} className="mr-1" />
            {t('logs.export')}
          </Button>

          <Button
            variant="ghost"
            size="sm"
            onClick={handleClear}
            className="h-8 text-xs text-muted-foreground hover:text-error"
          >
            <Trash2 size={14} className="mr-1" />
            {t('logs.clear')}
          </Button>
        </div>

        <div className="flex-1 overflow-auto font-mono text-xs">
          {filteredLogs.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-48 text-muted-foreground">
              <Bug size={32} className="mb-2 opacity-50" />
              <p className="text-sm">{t('logs.noLogs')}</p>
            </div>
          ) : (
            <div className="divide-y divide-border">
              {filteredLogs.map(log => (
                <div
                  key={log.id}
                  className={cn(
                    'px-4 py-2 hover:bg-muted/30 transition-colors',
                    log.level === 'error' && 'bg-error/5',
                    log.level === 'warn' && 'bg-warning/5'
                  )}
                >
                  <div className="flex items-start gap-2">
                    <div className="mt-0.5">{getLevelIcon(log.level)}</div>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 text-muted-foreground">
                        <span>{formatTime(log.timestamp)}</span>
                        <span className="text-accent">[{log.source}]</span>
                      </div>
                      <div className="text-foreground mt-0.5 break-all">{log.message}</div>
                      {log.details && (
                        <pre className="mt-1 p-2 bg-muted/50 rounded text-muted-foreground whitespace-pre-wrap break-all">
                          {log.details}
                        </pre>
                      )}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
