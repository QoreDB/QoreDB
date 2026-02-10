import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { motion } from 'framer-motion';
import {
  X,
  Trash2,
  FileCode,
  ChevronDown,
  ChevronRight,
  Table,
  Plus,
  Pencil,
  AlertTriangle,
  Eye,
  EyeOff,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import {
  getSandboxSession,
  getGroupedChanges,
  removeSandboxChange,
  clearSandboxChanges,
  clearTableChanges,
  subscribeSandbox,
  getSandboxPreferences,
  subscribeSandboxPreferences,
} from '@/lib/sandboxStore';
import { SandboxChangeGroup } from '@/lib/sandboxTypes';
import { ChangeItem } from './ChangeItem';
import { cn } from '@/lib/utils';
import { Environment } from '@/lib/tauri';

interface ChangesPanelProps {
  sessionId: string;
  isOpen: boolean;
  onClose: () => void;
  onGenerateSQL: () => void;
  environment?: Environment;
}

export function ChangesPanel({
  sessionId,
  isOpen,
  onClose,
  onGenerateSQL,
  environment = 'development',
}: ChangesPanelProps) {
  const { t } = useTranslation();
  const [groups, setGroups] = useState<SandboxChangeGroup[]>([]);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [totalChanges, setTotalChanges] = useState(0);
  const [pageSize, setPageSize] = useState(() => getSandboxPreferences().panelPageSize);
  const [visibleCounts, setVisibleCounts] = useState<Record<string, number>>({});
  const [expandedChanges, setExpandedChanges] = useState<Set<string>>(new Set());

  // Load and subscribe to changes
  useEffect(() => {
    const loadChanges = () => {
      const session = getSandboxSession(sessionId);
      const grouped = getGroupedChanges(sessionId);
      setGroups(grouped);
      setTotalChanges(session.changes.length);

      // Auto-expand first group
      if (grouped.length > 0 && expandedGroups.size === 0) {
        setExpandedGroups(new Set([grouped[0].displayName]));
      }

      setVisibleCounts(prev => {
        const next = { ...prev };
        for (const group of grouped) {
          if (!next[group.displayName]) {
            next[group.displayName] = pageSize;
          }
        }
        return next;
      });
    };

    loadChanges();

    const unsubscribe = subscribeSandbox(changedSessionId => {
      if (changedSessionId === sessionId) {
        loadChanges();
      }
    });

    return unsubscribe;
  }, [sessionId, expandedGroups.size, pageSize]);

  useEffect(() => {
    const unsubscribe = subscribeSandboxPreferences(prefs => {
      setPageSize(prefs.panelPageSize);
      setVisibleCounts(prev => {
        const next = { ...prev };
        for (const group of groups) {
          const current = next[group.displayName] ?? prefs.panelPageSize;
          next[group.displayName] = Math.max(current, prefs.panelPageSize);
        }
        return next;
      });
    });
    return unsubscribe;
  }, [groups]);

  const toggleGroup = useCallback((groupName: string) => {
    setExpandedGroups(prev => {
      const next = new Set(prev);
      if (next.has(groupName)) {
        next.delete(groupName);
      } else {
        next.add(groupName);
      }
      return next;
    });
  }, []);

  const handleUndo = useCallback(
    (changeId: string) => {
      removeSandboxChange(sessionId, changeId);
    },
    [sessionId]
  );

  const handleClearAll = useCallback(() => {
    clearSandboxChanges(sessionId);
  }, [sessionId]);

  const handleClearTable = useCallback(
    (group: SandboxChangeGroup) => {
      clearTableChanges(sessionId, group.namespace, group.tableName);
    },
    [sessionId]
  );

  const handleLoadMore = useCallback(
    (groupName: string) => {
      setVisibleCounts(prev => ({
        ...prev,
        [groupName]: (prev[groupName] ?? pageSize) + pageSize,
      }));
    },
    [pageSize]
  );

  const handleToggleDetails = useCallback((changeId: string) => {
    setExpandedChanges(prev => {
      const next = new Set(prev);
      if (next.has(changeId)) {
        next.delete(changeId);
      } else {
        next.add(changeId);
      }
      return next;
    });
  }, []);

  const handleToggleAll = useCallback(() => {
    const allChangeIds = new Set<string>();
    groups.forEach(group => group.changes.forEach(change => allChangeIds.add(change.id)));
    setExpandedChanges(prev => (prev.size === allChangeIds.size ? new Set() : allChangeIds));
  }, [groups]);

  return (
    <>
      <Dialog open={isOpen} onOpenChange={open => !open && onClose()} modal={false}>
        <DialogContent
          className={cn(
            'fixed right-0 top-0 h-full w-80 max-w-none p-0 border-l border-border gap-0',
            'left-auto translate-x-0 translate-y-0 rounded-none sm:rounded-none',
            '[&>button.absolute]:hidden'
          )}
          overlayClassName="bg-background/40 backdrop-blur-sm"
        >
          <motion.div
            initial={{ x: '100%' }}
            animate={{ x: 0 }}
            transition={{ type: 'spring', stiffness: 320, damping: 32 }}
            className={cn('h-full w-full bg-background shadow-xl z-40 flex flex-col')}
          >
            <DialogHeader className="flex flex-row items-center justify-between px-4 py-3 border-b border-border bg-muted/30 space-y-0 text-left">
              <div>
                <DialogTitle className="text-base font-semibold text-foreground">
                  {t('sandbox.changes.title')}
                </DialogTitle>
                <p className="text-xs text-muted-foreground">
                  {totalChanges === 0
                    ? t('sandbox.changes.empty')
                    : t('sandbox.changes.count', { count: totalChanges })}
                </p>
              </div>
              <Button variant="ghost" size="icon" onClick={onClose} className="h-8 w-8">
                <X size={16} />
              </Button>
            </DialogHeader>

            {(environment === 'staging' || environment === 'production') && (
              <div
                className={cn(
                  'flex items-start gap-2 px-4 py-3 text-xs border-b border-border',
                  environment === 'production'
                    ? 'bg-error/10 text-error'
                    : 'bg-warning/10 text-warning'
                )}
              >
                <AlertTriangle size={14} className="mt-0.5" />
                <span>
                  {environment === 'production'
                    ? t('sandbox.envWarningProduction')
                    : t('sandbox.envWarningStaging')}
                </span>
              </div>
            )}

            {/* Actions - avec labels explicites pour actions sensibles */}
            <div className="flex gap-2 px-4 py-2 border-b border-border">
              <Button
                variant="outline"
                size="sm"
                className="flex-1 h-8"
                onClick={onGenerateSQL}
                disabled={totalChanges === 0}
              >
                <FileCode size={14} className="mr-1.5" />
                {t('sandbox.generateSQL')}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                className="h-8 px-2 text-muted-foreground hover:text-foreground"
                onClick={handleToggleAll}
                disabled={totalChanges === 0}
                title={t('sandbox.changes.toggleAll')}
              >
                {expandedChanges.size > 0 ? <EyeOff size={14} /> : <Eye size={14} />}
              </Button>
              {/* Action destructive avec label explicite */}
              <Button
                variant="ghost"
                size="sm"
                className="h-8 px-2 text-error hover:text-error hover:bg-error/10"
                onClick={handleClearAll}
                disabled={totalChanges === 0}
              >
                <Trash2 size={14} className="mr-1" />
                <span className="text-xs">{t('sandbox.changes.clearAll')}</span>
              </Button>
            </div>

            {/* Changes List */}
            <ScrollArea className="flex-1 min-h-0">
              <div className="p-4 space-y-3">
                {groups.length === 0 ? (
                  <div className="flex flex-col items-center justify-center py-8 text-muted-foreground">
                    <FileCode size={32} className="mb-2 opacity-50" />
                    <p className="text-sm">{t('sandbox.changes.noChanges')}</p>
                    <p className="text-xs mt-1">{t('sandbox.changes.noChangesHint')}</p>
                  </div>
                ) : (
                  groups.map(group => {
                    const isExpanded = expandedGroups.has(group.displayName);
                    const visibleCount = visibleCounts[group.displayName] ?? pageSize;
                    const visibleChanges = group.changes.slice(0, visibleCount);
                    return (
                      <div
                        key={group.displayName}
                        className="border border-border rounded-lg overflow-hidden"
                      >
                        {/* Group Header */}
                        <button
                          type="button"
                          className="w-full flex items-center gap-2 px-3 py-2 bg-muted/30 hover:bg-muted/50 transition-colors"
                          onClick={() => toggleGroup(group.displayName)}
                        >
                          {isExpanded ? (
                            <ChevronDown size={14} className="text-muted-foreground" />
                          ) : (
                            <ChevronRight size={14} className="text-muted-foreground" />
                          )}
                          <Table size={14} className="text-accent" />
                          <span className="flex-1 text-sm font-medium text-left truncate">
                            {group.displayName}
                          </span>
                          <div className="flex items-center gap-1">
                            {group.counts.insert > 0 && (
                              <span className="flex items-center gap-0.5 px-1.5 py-0.5 rounded text-[10px] bg-success/10 text-success">
                                <Plus size={8} />
                                {group.counts.insert}
                              </span>
                            )}
                            {group.counts.update > 0 && (
                              <span className="flex items-center gap-0.5 px-1.5 py-0.5 rounded text-[10px] bg-warning/10 text-warning">
                                <Pencil size={8} />
                                {group.counts.update}
                              </span>
                            )}
                            {group.counts.delete > 0 && (
                              <span className="flex items-center gap-0.5 px-1.5 py-0.5 rounded text-[10px] bg-error/10 text-error">
                                <Trash2 size={8} />
                                {group.counts.delete}
                              </span>
                            )}
                          </div>
                        </button>

                        {/* Group Content */}
                        {isExpanded && (
                          <div className="p-2 space-y-2 bg-background">
                            {/* Table Actions */}
                            <div className="flex justify-end">
                              <Button
                                variant="ghost"
                                size="sm"
                                className="h-6 text-xs text-muted-foreground hover:text-error"
                                onClick={() => handleClearTable(group)}
                              >
                                <Trash2 size={10} className="mr-1" />
                                {t('sandbox.changes.clearTable')}
                              </Button>
                            </div>

                            {/* Changes */}
                            {visibleChanges.map(change => (
                              <ChangeItem
                                key={change.id}
                                change={change}
                                onUndo={handleUndo}
                                compact
                                expanded={expandedChanges.has(change.id)}
                                onToggleDetails={handleToggleDetails}
                              />
                            ))}

                            {group.changes.length > visibleCount && (
                              <div className="flex items-center justify-between text-xs text-muted-foreground px-1">
                                <span>
                                  {t('sandbox.changes.showing', {
                                    shown: visibleChanges.length,
                                    total: group.changes.length,
                                  })}
                                </span>
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  className="h-6 px-2 text-xs"
                                  onClick={() => handleLoadMore(group.displayName)}
                                >
                                  {t('sandbox.changes.loadMore')}
                                </Button>
                              </div>
                            )}
                          </div>
                        )}
                      </div>
                    );
                  })
                )}
              </div>
            </ScrollArea>
          </motion.div>
        </DialogContent>
      </Dialog>
    </>
  );
}
