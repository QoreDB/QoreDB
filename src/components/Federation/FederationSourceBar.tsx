// SPDX-License-Identifier: BUSL-1.1

import { ChevronDown, Loader2, Plus, RefreshCw } from 'lucide-react';
import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { ScrollArea } from '@/components/ui/scroll-area';
import { DBTree } from '@/components/Tree/DBTree';
import { DRIVER_ICONS, DRIVER_LABELS, type Driver } from '@/lib/drivers';
import type { FederationSource } from '@/lib/federation';
import type { Namespace } from '@/lib/tauri';
import { cn } from '@/lib/utils';

interface FederationSourceBarProps {
  sources: FederationSource[];
  loading: boolean;
  onRefresh: () => void;
  onAddSource: () => void;
  onInsertTable: (alias: string, ns: Namespace, table: string) => void;
}

export function FederationSourceBar({
  sources,
  loading,
  onRefresh,
  onAddSource,
  onInsertTable,
}: FederationSourceBarProps) {
  const { t } = useTranslation();
  const [openPopover, setOpenPopover] = useState<string | null>(null);

  const handleTableSelect = useCallback(
    (alias: string, ns: Namespace, table: string) => {
      onInsertTable(alias, ns, table);
      setOpenPopover(null);
    },
    [onInsertTable]
  );

  return (
    <div className="flex items-center gap-1.5 px-3 py-1.5 border-b border-border bg-muted/10 overflow-x-auto no-scrollbar shrink-0">
      <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground shrink-0 mr-1">
        {t('federation.sourcesLabel')}
      </span>

      {loading && sources.length === 0 ? (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 size={12} className="animate-spin" />
          <span>{t('federation.loadingSources')}</span>
        </div>
      ) : sources.length === 0 ? (
        <button
          onClick={onAddSource}
          className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md border border-dashed border-border text-xs text-muted-foreground hover:text-foreground hover:border-accent/40 hover:bg-accent/5 transition-all"
        >
          <Plus size={12} />
          {t('federation.connectFirst')}
        </button>
      ) : (
        <>
          {sources.map(source => {
            const driver = source.driver as Driver;
            const isOpen = openPopover === source.alias;

            return (
              <Popover
                key={source.alias}
                open={isOpen}
                onOpenChange={open => setOpenPopover(open ? source.alias : null)}
              >
                <PopoverTrigger asChild>
                  <button
                    className={cn(
                      'flex items-center gap-1.5 pl-2 pr-2 py-1 rounded-md border text-xs transition-all shrink-0',
                      'hover:border-accent/30 hover:bg-accent/5',
                      isOpen
                        ? 'bg-accent/10 border-accent/30 text-foreground ring-1 ring-accent/20'
                        : 'bg-background border-border text-foreground'
                    )}
                  >
                    <div className="w-3.5 h-3.5 rounded-sm overflow-hidden shrink-0">
                      <img
                        src={`/databases/${DRIVER_ICONS[driver]}`}
                        alt={DRIVER_LABELS[driver] || driver}
                        className="w-full h-full object-contain"
                      />
                    </div>
                    <span className="font-medium truncate max-w-24">{source.display_name}</span>
                    <span className="font-mono text-[10px] text-muted-foreground/70">
                      {source.alias}
                    </span>
                    <ChevronDown
                      size={10}
                      className={cn(
                        'text-muted-foreground transition-transform ml-0.5',
                        isOpen && 'rotate-180'
                      )}
                    />
                  </button>
                </PopoverTrigger>
                <PopoverContent className="w-80 p-0" align="start" sideOffset={6}>
                  <div className="flex items-center gap-2 px-3 py-2 border-b border-border bg-muted/30">
                    <div className="w-4 h-4 rounded-sm overflow-hidden shrink-0">
                      <img
                        src={`/databases/${DRIVER_ICONS[driver]}`}
                        alt={DRIVER_LABELS[driver]}
                        className="w-full h-full object-contain"
                      />
                    </div>
                    <div className="flex-1 min-w-0">
                      <span className="text-xs font-medium">{source.display_name}</span>
                      <span className="text-[10px] text-muted-foreground font-mono ml-1.5">
                        {source.alias}
                      </span>
                    </div>
                  </div>
                  <p className="text-[10px] text-muted-foreground px-3 py-1.5 bg-accent/5 border-b border-border">
                    {t('federation.clickTableToInsert')}
                  </p>
                  <ScrollArea className="max-h-[360px]">
                    <div className="p-2">
                      <DBTree
                        connectionId={source.session_id}
                        driver={source.driver}
                        onTableSelect={(ns, table) =>
                          handleTableSelect(source.alias, ns, table)
                        }
                      />
                    </div>
                  </ScrollArea>
                </PopoverContent>
              </Popover>
            );
          })}
        </>
      )}

      <div className="flex-1" />

      <Button
        variant="ghost"
        size="icon"
        className="h-6 w-6 shrink-0 text-muted-foreground hover:text-foreground"
        onClick={onRefresh}
        disabled={loading}
        title={t('federation.refreshSources')}
      >
        {loading ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}
      </Button>

      <Button
        variant="ghost"
        size="icon"
        className="h-6 w-6 shrink-0 text-muted-foreground hover:text-foreground"
        onClick={onAddSource}
        title={t('federation.addSource')}
      >
        <Plus size={12} />
      </Button>
    </div>
  );
}
