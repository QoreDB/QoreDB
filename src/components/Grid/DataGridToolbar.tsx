import { RefObject } from "react";
import { Table, Column } from "@tanstack/react-table";
import { useTranslation } from "react-i18next";
import {
  Search,
  X,
  Eye,
  EyeOff,
  ChevronDown,
  ListFilter,
  Database,
  Check,
  Copy,
  FileSpreadsheet,
  FileJson,
  Code2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';
import { RowData } from './utils/dataGridUtils';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';

interface DataGridToolbarProps {
  table: Table<RowData>;
  globalFilter: string;
  setGlobalFilter: (value: string) => void;
  searchInputRef: RefObject<HTMLInputElement | null>;
  copyToClipboard: (format: 'csv' | 'json' | 'sql') => void;
  onStreamingExport?: () => void;
  copied: boolean;
  showFilters: boolean;
  setShowFilters: (show: boolean) => void;
}

export function DataGridToolbar({
  table,
  globalFilter,
  setGlobalFilter,
  searchInputRef,
  copyToClipboard,
  onStreamingExport,
  copied,
  showFilters,
  setShowFilters,
}: DataGridToolbarProps) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center gap-2">
      {/* Global Search */}
      <div className="relative">
        <Search
          size={14}
          className="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground"
        />
        <Input
          ref={searchInputRef}
          type="text"
          placeholder={t('grid.searchPlaceholder')}
          value={globalFilter}
          onChange={e => setGlobalFilter(e.target.value)}
          className="h-7 max-w-50 pl-7 pr-7 text-xs"
        />
        {globalFilter && (
          <button
            onClick={() => setGlobalFilter('')}
            className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
          >
            <X size={12} />
          </button>
        )}
      </div>
      {/* Filter Toggle */}
      <Button
        variant="ghost"
        size="icon"
        className={cn('h-7 w-7', showFilters && 'bg-accent/20 text-accent')}
        onClick={() => setShowFilters(!showFilters)}
        title={t('grid.toggleFilters')}
      >
        <ListFilter size={14} />
      </Button>
      {/* Column Visibility */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="sm" className="h-7 px-2 text-xs">
            <Eye size={14} />
            <span className="ml-1">{t('grid.columns')}</span>
            <ChevronDown size={12} className="ml-1" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-48 max-h-64 overflow-auto">
          <DropdownMenuLabel className="text-xs">{t('grid.toggleColumns')}</DropdownMenuLabel>
          <DropdownMenuSeparator />
          {table
            .getAllLeafColumns()
            .filter((col: Column<RowData, unknown>) => col.id !== 'select')
            .map((column: Column<RowData, unknown>) => (
              <DropdownMenuItem
                key={column.id}
                onClick={e => {
                  e.preventDefault();
                  column.toggleVisibility();
                }}
                className="text-xs cursor-pointer group"
              >
                {column.getIsVisible() ? (
                  <Eye size={14} className="mr-2 text-muted-foreground" />
                ) : (
                  <EyeOff
                    size={14}
                    className="mr-2 text-accent group-hover:text-accent-foreground"
                  />
                )}
                {column.id}
              </DropdownMenuItem>
            ))}
        </DropdownMenuContent>
      </DropdownMenu>
      {/* Export Dropdown */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="sm" className="h-7 px-2 text-xs">
            {copied ? <Check size={14} className="text-green-500" /> : <Copy size={14} />}
            <span className="ml-1">Export</span>
            <ChevronDown size={12} className="ml-1" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-40">
          <DropdownMenuLabel className="text-xs">{t('grid.copyToClipboard')}</DropdownMenuLabel>
          <DropdownMenuItem
            onClick={() => {
              AnalyticsService.capture('export_used', {
                format: 'csv',
                destination: 'clipboard',
              });
              copyToClipboard('csv');
            }}
            className="text-xs"
          >
            <FileSpreadsheet size={14} className="mr-2" />
            CSV
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => {
              AnalyticsService.capture('export_used', {
                format: 'json',
                destination: 'clipboard',
              });
              copyToClipboard('json');
            }}
            className="text-xs"
          >
            <FileJson size={14} className="mr-2" />
            JSON
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => {
              AnalyticsService.capture('export_used', {
                format: 'sql',
                destination: 'clipboard',
              });
              copyToClipboard('sql');
            }}
            className="text-xs"
          >
            <Code2 size={14} className="mr-2" />
            SQL
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuLabel className="text-xs">{t('grid.streamingExport')}</DropdownMenuLabel>
          <DropdownMenuItem onClick={onStreamingExport} className="text-xs" disabled={!onStreamingExport}>
            <Database size={14} className="mr-2" />
            {t('grid.exportAllRows')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
