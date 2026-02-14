/**
 * DiffStatsBar - Statistics bar with counters and filters
 */
import { useTranslation } from 'react-i18next';
import { PlusCircle, MinusCircle, ArrowLeftRight, CheckCircle2, Eye, EyeOff } from 'lucide-react';
import { cn } from '@/lib/utils';
import { DiffStats, DiffRowStatus } from '@/lib/diffUtils';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

export type DiffFilter = 'all' | DiffRowStatus;

interface DiffStatsBarProps {
  stats: DiffStats;
  filter: DiffFilter;
  onFilterChange: (filter: DiffFilter) => void;
  showUnchanged: boolean;
  onShowUnchangedChange: (show: boolean) => void;
}

export function DiffStatsBar({
  stats,
  filter,
  onFilterChange,
  showUnchanged,
  onShowUnchangedChange,
}: DiffStatsBarProps) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center justify-between px-4 py-2 border-b border-border bg-muted/20">
      {/* Stats counters */}
      <div className="flex items-center gap-4 text-sm">
        <StatCounter
          icon={<PlusCircle size={14} />}
          count={stats.added}
          label={t('diff.added')}
          colorClass="text-success"
          active={filter === 'added'}
          onClick={() => onFilterChange(filter === 'added' ? 'all' : 'added')}
        />
        <StatCounter
          icon={<MinusCircle size={14} />}
          count={stats.removed}
          label={t('diff.removed')}
          colorClass="text-error"
          active={filter === 'removed'}
          onClick={() => onFilterChange(filter === 'removed' ? 'all' : 'removed')}
        />
        <StatCounter
          icon={<ArrowLeftRight size={14} />}
          count={stats.modified}
          label={t('diff.modified')}
          colorClass="text-warning"
          active={filter === 'modified'}
          onClick={() => onFilterChange(filter === 'modified' ? 'all' : 'modified')}
        />
        <StatCounter
          icon={<CheckCircle2 size={14} />}
          count={stats.unchanged}
          label={t('diff.unchanged')}
          colorClass="text-muted-foreground"
          active={filter === 'unchanged'}
          onClick={() => onFilterChange(filter === 'unchanged' ? 'all' : 'unchanged')}
        />
      </div>

      {/* Filters */}
      <div className="flex items-center gap-2">
        <Select value={filter} onValueChange={v => onFilterChange(v as DiffFilter)}>
          <SelectTrigger className="w-36 h-8 text-sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t('diff.filterAll')}</SelectItem>
            <SelectItem value="added">{t('diff.filterAdded')}</SelectItem>
            <SelectItem value="removed">{t('diff.filterRemoved')}</SelectItem>
            <SelectItem value="modified">{t('diff.filterModified')}</SelectItem>
            <SelectItem value="unchanged">{t('diff.filterUnchanged')}</SelectItem>
          </SelectContent>
        </Select>

        <Button
          variant="ghost"
          size="sm"
          onClick={() => onShowUnchangedChange(!showUnchanged)}
          className={cn('h-8', !showUnchanged && 'text-muted-foreground')}
        >
          {showUnchanged ? (
            <Eye size={14} className="mr-1.5" />
          ) : (
            <EyeOff size={14} className="mr-1.5" />
          )}
          {t('diff.showUnchanged')}
        </Button>
      </div>
    </div>
  );
}

interface StatCounterProps {
  icon: React.ReactNode;
  count: number;
  label: string;
  colorClass: string;
  active?: boolean;
  onClick?: () => void;
}

function StatCounter({ icon, count, label, colorClass, active, onClick }: StatCounterProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'flex items-center gap-1.5 px-2 py-1 rounded-md transition-colors',
        active ? 'bg-accent ring-1 ring-accent-foreground/20' : 'hover:bg-muted/50',
        colorClass
      )}
    >
      {icon}
      <span className="font-medium tabular-nums">{count}</span>
      <span className="text-xs opacity-80">{label}</span>
    </button>
  );
}
