/**
 * DataGrid Toolbar - Search, Columns visibility, Export
 */

import { RefObject } from 'react';
import { Table, Column } from '@tanstack/react-table';
import { useTranslation } from 'react-i18next';
import { 
  Search, 
  X, 
  Eye, 
  EyeOff, 
  ChevronDown, 
  Check, 
  Copy, 
  FileSpreadsheet, 
  FileJson, 
  Code2, 
  Download 
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
import { RowData } from './utils/dataGridUtils';

interface DataGridToolbarProps {
  table: Table<RowData>;
  globalFilter: string;
  setGlobalFilter: (value: string) => void;
  searchInputRef: RefObject<HTMLInputElement | null>;
  copyToClipboard: (format: 'csv' | 'json' | 'sql') => void;
  exportToFile: (format: 'csv' | 'json') => void;
  copied: string | null;
}

export function DataGridToolbar({
  table,
  globalFilter,
  setGlobalFilter,
  searchInputRef,
  copyToClipboard,
  exportToFile,
  copied,
}: DataGridToolbarProps) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center gap-2">
      {/* Global Search */}
      <div className="relative">
        <Search size={14} className="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground" />
        <Input
          ref={searchInputRef}
          type="text"
          placeholder={t('grid.searchPlaceholder')}
          value={globalFilter}
          onChange={(e) => setGlobalFilter(e.target.value)}
          className="h-7 w-40 pl-7 pr-7 text-xs"
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
          {table.getAllLeafColumns()
            .filter((col: Column<RowData, unknown>) => col.id !== 'select')
            .map((column: Column<RowData, unknown>) => (
              <DropdownMenuItem
                key={column.id}
                onClick={(e) => {
                  e.preventDefault();
                  column.toggleVisibility();
                }}
                className="text-xs cursor-pointer"
              >
                {column.getIsVisible() ? (
                  <Eye size={14} className="mr-2 text-accent" />
                ) : (
                  <EyeOff size={14} className="mr-2 text-muted-foreground" />
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
          <DropdownMenuItem onClick={() => copyToClipboard('csv')} className="text-xs">
            <FileSpreadsheet size={14} className="mr-2" />
            CSV
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => copyToClipboard('json')} className="text-xs">
            <FileJson size={14} className="mr-2" />
            JSON
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => copyToClipboard('sql')} className="text-xs">
            <Code2 size={14} className="mr-2" />
            SQL
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuLabel className="text-xs">{t('grid.downloadToFile')}</DropdownMenuLabel>
          <DropdownMenuItem onClick={() => exportToFile('csv')} className="text-xs">
            <Download size={14} className="mr-2" />
            CSV
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => exportToFile('json')} className="text-xs">
            <Download size={14} className="mr-2" />
            JSON
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
