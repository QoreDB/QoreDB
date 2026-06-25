// SPDX-License-Identifier: Apache-2.0

import { Check, ChevronDown, Download } from 'lucide-react';
import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { type AuditExportFormat, exportAuditLog } from '../../lib/tauri/interceptor';
import { Button } from '../ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';

const FORMAT_MIME: Record<AuditExportFormat, string> = {
  json: 'application/json',
  jsonl: 'application/x-ndjson',
  csv: 'text/csv',
};

const FORMAT_LABEL: Record<AuditExportFormat, string> = {
  json: 'JSON',
  jsonl: 'JSONL',
  csv: 'CSV',
};

interface AuditExportMenuProps {
  disabled?: boolean;
}

export function AuditExportMenu({ disabled }: AuditExportMenuProps) {
  const { t } = useTranslation();
  const [fromDisk, setFromDisk] = useState(true);
  const [busy, setBusy] = useState(false);

  const download = useCallback(
    async (format: AuditExportFormat) => {
      try {
        setBusy(true);
        const content = await exportAuditLog(format, fromDisk);
        const blob = new Blob([content], { type: FORMAT_MIME[format] });
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement('a');
        anchor.href = url;
        anchor.download = `qoredb-audit-log-${new Date().toISOString().split('T')[0]}.${format}`;
        anchor.click();
        URL.revokeObjectURL(url);
      } catch (err) {
        console.error('Failed to export audit log:', err);
      } finally {
        setBusy(false);
      }
    },
    [fromDisk]
  );

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="sm" disabled={disabled || busy}>
          <Download className="w-4 h-4 mr-1" />
          {t('interceptor.audit.export.label')}
          <ChevronDown className="w-3 h-3 ml-1 opacity-60" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-52">
        <DropdownMenuLabel>{t('interceptor.audit.export.format')}</DropdownMenuLabel>
        {(['json', 'jsonl', 'csv'] as AuditExportFormat[]).map(format => (
          <DropdownMenuItem key={format} onClick={() => download(format)}>
            {FORMAT_LABEL[format]}
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onSelect={event => {
            event.preventDefault();
            setFromDisk(prev => !prev);
          }}
        >
          <span className="flex items-center justify-between w-full">
            <span>{t('interceptor.audit.export.fromDisk')}</span>
            {fromDisk && <Check className="w-3 h-3" />}
          </span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
