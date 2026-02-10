/**
 * DiffConfigPanel - Configuration panel for key columns and compare action
 */
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Key, GitCompare, Loader2, Sparkles, X, AlertTriangle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { ColumnInfo, Namespace, describeTable } from '@/lib/tauri';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import {
  TooltipContent,
  TooltipProvider,
  TooltipRoot,
  TooltipTrigger,
} from '@/components/ui/tooltip';

interface DiffConfigPanelProps {
  leftColumns?: ColumnInfo[];
  rightColumns?: ColumnInfo[];
  keyColumns: string[];
  onKeyColumnsChange: (columns: string[]) => void;
  onCompare: () => void;
  comparing: boolean;
  canCompare: boolean;
  compareBlockedText?: string | null;
  compareWarningText?: string | null;
  leftSessionId?: string;
  rightSessionId?: string;
  leftNamespace?: Namespace;
  rightNamespace?: Namespace;
  leftTableName?: string;
  rightTableName?: string;
}

export function DiffConfigPanel({
  leftColumns,
  rightColumns,
  keyColumns,
  onKeyColumnsChange,
  onCompare,
  comparing,
  canCompare,
  compareBlockedText,
  compareWarningText,
  leftSessionId,
  rightSessionId,
  leftNamespace,
  rightNamespace,
  leftTableName,
  rightTableName,
}: DiffConfigPanelProps) {
  const { t } = useTranslation();
  const [autoDetectPK, setAutoDetectPK] = useState(true);
  const [detectedPK, setDetectedPK] = useState<string[]>([]);
  const [loadingPK, setLoadingPK] = useState(false);
  type DescribeTableResponse = Awaited<ReturnType<typeof describeTable>>;

  // Common columns between left and right
  const commonColumns =
    leftColumns && rightColumns
      ? leftColumns.filter(lc => rightColumns.some(rc => rc.name === lc.name))
      : [];

  // Auto-detect primary key
  useEffect(() => {
    if (!autoDetectPK) {
      setDetectedPK([]);
      return;
    }

    const canDetectLeft = Boolean(leftSessionId && leftNamespace && leftTableName);
    const canDetectRight = Boolean(rightSessionId && rightNamespace && rightTableName);

    if (!canDetectLeft && !canDetectRight) {
      setDetectedPK([]);
      return;
    }

    setLoadingPK(true);

    const leftPromise: Promise<DescribeTableResponse> = canDetectLeft
      ? describeTable(leftSessionId!, leftNamespace!, leftTableName!)
      : Promise.resolve({ success: false });
    const rightPromise: Promise<DescribeTableResponse> = canDetectRight
      ? describeTable(rightSessionId!, rightNamespace!, rightTableName!)
      : Promise.resolve({ success: false });

    Promise.all([leftPromise, rightPromise])
      .then(([leftRes, rightRes]) => {
        const leftPk =
          leftRes.success && leftRes.schema?.primary_key ? leftRes.schema.primary_key : [];
        const rightPk =
          rightRes.success && rightRes.schema?.primary_key ? rightRes.schema.primary_key : [];

        let pk = leftPk;
        if (leftPk.length > 0 && rightPk.length > 0) {
          const intersection = leftPk.filter(col => rightPk.includes(col));
          pk = intersection.length > 0 ? intersection : leftPk;
        } else if (rightPk.length > 0) {
          pk = rightPk;
        }

        setDetectedPK(pk);
        if (keyColumns.length === 0 && pk.length > 0) {
          onKeyColumnsChange(pk);
        }
      })
      .catch(() => setDetectedPK([]))
      .finally(() => setLoadingPK(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    autoDetectPK,
    leftSessionId,
    rightSessionId,
    leftNamespace,
    rightNamespace,
    leftTableName,
    rightTableName,
  ]);

  const toggleKeyColumn = (columnName: string) => {
    if (keyColumns.includes(columnName)) {
      onKeyColumnsChange(keyColumns.filter(c => c !== columnName));
    } else {
      onKeyColumnsChange([...keyColumns, columnName]);
    }
  };

  const clearKeyColumns = () => {
    onKeyColumnsChange([]);
  };

  const applyDetectedPK = () => {
    onKeyColumnsChange(detectedPK);
  };

  return (
    <div className="flex flex-col gap-4 p-4 border border-border rounded-lg bg-muted/30">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Key size={16} className="text-muted-foreground" />
          <span className="text-sm font-medium">{t('diff.keyColumns')}</span>
        </div>
        {keyColumns.length > 0 && (
          <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={clearKeyColumns}>
            <X size={12} className="mr-1" />
            {t('common.clear')}
          </Button>
        )}
      </div>

      {/* Auto-detect PK option */}
      <div className="flex items-center gap-2">
        <Checkbox
          id="auto-detect-pk"
          checked={autoDetectPK}
          onCheckedChange={checked => setAutoDetectPK(checked === true)}
        />
        <Label htmlFor="auto-detect-pk" className="text-sm cursor-pointer">
          {t('diff.autoDetectPK')}
        </Label>
        {loadingPK && <Loader2 size={14} className="animate-spin text-muted-foreground" />}
        {detectedPK.length > 0 && !loadingPK && (
          <TooltipProvider>
            <TooltipRoot>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-6 px-2 text-xs text-success"
                  onClick={applyDetectedPK}
                >
                  <Sparkles size={12} className="mr-1" />
                  {t('diff.applyDetectedPK', { columns: detectedPK.join(', ') })}
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                <p>{t('diff.detectedPKHint')}</p>
              </TooltipContent>
            </TooltipRoot>
          </TooltipProvider>
        )}
      </div>

      {/* Column selection */}
      {commonColumns.length > 0 ? (
        <div className="flex flex-wrap gap-2">
          {commonColumns.map(col => {
            const isKey = keyColumns.includes(col.name);
            const isPK = detectedPK.includes(col.name);
            return (
              <button
                key={col.name}
                type="button"
                onClick={() => toggleKeyColumn(col.name)}
                className={cn(
                  'inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-full border transition-colors',
                  isKey
                    ? 'bg-primary text-primary-foreground border-primary'
                    : 'bg-muted/50 text-muted-foreground border-border hover:bg-muted hover:text-foreground'
                )}
              >
                {isPK && <Key size={10} className="shrink-0" />}
                {col.name}
              </button>
            );
          })}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">{t('diff.keyColumnsHint')}</p>
      )}

      {compareBlockedText && (
        <div className="flex items-start gap-2 text-sm text-warning">
          <AlertTriangle size={14} className="mt-0.5" />
          <span>{compareBlockedText}</span>
        </div>
      )}

      {compareWarningText && (
        <div className="flex items-start gap-2 text-sm text-warning">
          <AlertTriangle size={14} className="mt-0.5" />
          <span>{compareWarningText}</span>
        </div>
      )}

      {/* Compare button */}
      <Button
        onClick={onCompare}
        disabled={!canCompare || comparing}
        size="lg"
        className="w-full mt-2"
      >
        {comparing ? (
          <>
            <Loader2 size={18} className="mr-2 animate-spin" />
            {t('diff.comparing')}
          </>
        ) : (
          <>
            <GitCompare size={18} className="mr-2" />
            {t('diff.compare')}
          </>
        )}
      </Button>
    </div>
  );
}
