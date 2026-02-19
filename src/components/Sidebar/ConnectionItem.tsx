// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, ChevronRight, Loader2 } from 'lucide-react';
import { cn } from '@/lib/utils';
import { DRIVER_ICONS, DRIVER_LABELS, type Driver } from '../../lib/drivers';
import { ENVIRONMENT_CONFIG } from '../../lib/environment';
import type { SavedConnection } from '../../lib/tauri';
import { ConnectionContextMenu } from '../Connection/ConnectionContextMenu';
import { ConnectionMenu } from '../Connection/ConnectionMenu';

interface ConnectionItemProps {
  connection: SavedConnection;
  isSelected: boolean;
  isExpanded: boolean;
  isConnected?: boolean;
  isConnecting?: boolean;
  isFavorite?: boolean;
  onSelect: () => void;
  onToggleFavorite: () => void;
  onEdit: (connection: SavedConnection, password: string) => void;
  onDeleted: () => void;
}

export function ConnectionItem({
  connection,
  isSelected,
  isExpanded,
  isConnected,
  isConnecting,
  isFavorite,
  onSelect,
  onToggleFavorite,
  onEdit,
  onDeleted,
}: ConnectionItemProps) {
  const driver = connection.driver as Driver;
  const iconSrc = `/databases/${DRIVER_ICONS[driver]}`;
  const env = connection.environment || 'development';
  const envConfig = ENVIRONMENT_CONFIG[env];

  return (
    <ConnectionContextMenu
      connection={connection}
      onEdit={onEdit}
      onDeleted={onDeleted}
      isFavorite={isFavorite}
      onToggleFavorite={onToggleFavorite}
    >
      <div
        className={cn(
          'group relative flex items-center transition-all rounded-md',
          // État: Sélectionné mais pas connecté
          isSelected && !isConnected && 'bg-muted text-foreground',
          // État: Connecté (actif)
          isSelected && isConnected && 'bg-(--q-accent-soft) text-(--q-accent) font-medium',
          // État: Déplié (expanded) mais pas sélectionné
          !isSelected && isExpanded && 'bg-muted/50 text-foreground',
          // État: Normal (non sélectionné, non déplié)
          !isSelected &&
            !isExpanded &&
            'text-muted-foreground hover:bg-accent/10 hover:text-accent-foreground'
        )}
      >
        <button
          type="button"
          className={cn(
            'flex-1 flex items-center gap-2 px-2 py-1.5 text-sm select-none text-inherit rounded-l-md'
          )}
          onClick={onSelect}
          disabled={isConnecting}
        >
          <div className="relative shrink-0">
            <div className="w-4 h-4 rounded-sm overflow-hidden bg-background/50 p-0.5">
              <img
                src={iconSrc}
                alt={DRIVER_LABELS[driver]}
                className="w-full h-full object-contain"
              />
            </div>
            {isConnected && !isConnecting && (
              <span className="absolute -bottom-0.5 -right-0.5 w-1.5 h-1.5 rounded-full bg-success ring-1 ring-background" />
            )}
          </div>

          <span className="flex-1 truncate text-left min-w-0">{connection.name}</span>

          {env !== 'development' && (
            <span
              className="px-1.5 py-0.5 text-[10px] font-bold rounded"
              style={{
                backgroundColor: envConfig.bgSoft,
                color: envConfig.color,
              }}
            >
              {envConfig.labelShort}
            </span>
          )}

          {isConnecting && <Loader2 size={14} className="animate-spin text-muted-foreground" />}

          <div className="relative shrink-0 w-6 h-6">
            <div
              className={cn(
                'absolute inset-0 flex items-center justify-center text-muted-foreground/50 transition-opacity group-hover:opacity-0',
                isExpanded && 'transform rotate-90'
              )}
            >
              {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            </div>
          </div>
        </button>

        <div className="absolute right-0 shrink-0 w-6 h-6 flex items-center justify-center opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto">
          <ConnectionMenu
            connection={connection}
            onEdit={onEdit}
            onDeleted={onDeleted}
            isFavorite={isFavorite}
            onToggleFavorite={onToggleFavorite}
          />
        </div>
      </div>
    </ConnectionContextMenu>
  );
}
