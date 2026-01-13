import { SavedConnection } from '../../lib/tauri';
import { Loader2, ChevronRight, ChevronDown } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Driver, DRIVER_ICONS, DRIVER_LABELS } from '../../lib/drivers';
import { ConnectionMenu } from '../Connection/ConnectionMenu';

interface ConnectionItemProps {
  connection: SavedConnection;
  isSelected: boolean;
  isExpanded: boolean;
  isConnected?: boolean;
  isConnecting?: boolean;
  onSelect: () => void;
  onEdit: (connection: SavedConnection, password: string) => void;
  onDeleted: () => void;
}

export function ConnectionItem({ 
  connection, 
  isSelected, 
  isExpanded, 
  isConnected,
  isConnecting,
  onSelect,
  onEdit,
  onDeleted
}: ConnectionItemProps) {
  const driver = connection.driver as Driver;
  const iconSrc = `/databases/${DRIVER_ICONS[driver]}`;

  return (
    <div className={cn(
      "group flex items-center rounded-md transition-all",
      isSelected && !isConnected && "bg-muted text-foreground",
      isSelected && isConnected && "bg-[var(--q-accent-soft)] text-[var(--q-accent)] font-medium",
      !isSelected && "text-muted-foreground hover:bg-accent/10 hover:text-accent-foreground"
    )}>
      <button
        className={cn(
          "flex-1 flex items-center gap-2 px-2 py-1.5 text-sm rounded-l-md select-none text-inherit",
        )}
        onClick={onSelect}
        disabled={isConnecting}
      >
        <div className="shrink-0 w-4 h-4 rounded-sm overflow-hidden bg-background/50 p-0.5">
          <img 
            src={iconSrc} 
            alt={DRIVER_LABELS[driver]} 
            className="w-full h-full object-contain"
          />
        </div>
        
        <span className="flex-1 truncate text-left">
          {connection.name}
        </span>
        
        {isConnecting ? (
          <Loader2 size={14} className="animate-spin text-muted-foreground" />
        ) : isConnected && !isConnecting ? (
          <span className="w-2 h-2 rounded-full bg-success shadow-sm shadow-success/50" />
        ) : null}
        
        <div className={cn("text-muted-foreground/50", isExpanded && "transform rotate-90")}>
          {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </div>
      </button>

      <ConnectionMenu 
        connection={connection}
        onEdit={onEdit}
        onDeleted={onDeleted}
      />
    </div>
  );
}
