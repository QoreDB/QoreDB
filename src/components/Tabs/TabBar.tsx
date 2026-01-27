import { X, Plus, FileCode, Table, Settings, Database } from 'lucide-react';
import { cn } from '@/lib/utils';
import { useTranslation } from 'react-i18next';

export interface TabItem {
  id: string;
  title: string;
  type: 'query' | 'table' | 'database' | 'settings';
}

interface TabBarProps {
  tabs?: TabItem[];
  activeId?: string;
  onSelect?: (id: string) => void;
  onClose?: (id: string) => void;
  onNew?: () => void;
}

export function TabBar({ 
  tabs = [], 
  activeId, 
  onSelect, 
  onClose, 
  onNew 
}: TabBarProps) {
  const { t } = useTranslation();

  const getTabIcon = (type: TabItem['type']) => {
    switch (type) {
      case 'query': return <FileCode size={14} />;
      case 'table': return <Table size={14} />;
      case 'database': return <Database size={14} />;
      case 'settings': return <Settings size={14} />;
    }
  };

  return (
    <div className="flex items-center w-full bg-muted/30 border-b border-border h-10 select-none pl-1 gap-1 overflow-x-auto overflow-y-hidden no-scrollbar">
      {tabs.map(tab => (
        <button
          key={tab.id}
          className={cn(
            'group flex items-center gap-2 pl-3 pr-2 py-1.5 min-w-35 max-w-50 h-8.5 text-xs rounded-t-md border-t border-x border-transparent mt-1.25 transition-all relative',
            activeId === tab.id
              ? 'bg-background text-foreground font-medium border-border -mb-px shadow-sm z-10'
              : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground'
          )}
          onClick={() => onSelect?.(tab.id)}
          onMouseDown={e => {
            if (e.button === 1) {
              e.preventDefault();
              onClose?.(tab.id);
            }
          }}
          title={tab.title}
        >
          <span className="shrink-0 opacity-70">{getTabIcon(tab.type)}</span>
          <span className="truncate flex-1 text-left">{tab.title}</span>
          <span
            className={cn(
              'opacity-0 group-hover:opacity-100 p-0.5 rounded-sm hover:bg-muted-foreground/20 text-muted-foreground transition-all shrink-0',
              'cursor-pointer'
            )}
            onClick={e => {
              e.stopPropagation();
              onClose?.(tab.id);
            }}
          >
            <X size={12} />
          </span>
        </button>
      ))}
      <button
        className="flex items-center justify-center w-8 h-8 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground ml-1 transition-colors"
        onClick={onNew}
        title={t('tabs.newQuery')}
      >
        <Plus size={16} />
      </button>
    </div>
  );
}

