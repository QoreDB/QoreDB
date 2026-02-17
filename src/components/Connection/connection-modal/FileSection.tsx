// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import { open, save } from '@tauri-apps/plugin-dialog';
import { File, FolderOpen, Database, Plus } from 'lucide-react';

import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

import type { ConnectionFormData } from './types';

interface FileSectionProps {
  formData: ConnectionFormData;
  onChange: (field: keyof ConnectionFormData, value: string | number | boolean) => void;
}

export function FileSection({ formData, onChange }: FileSectionProps) {
  const { t } = useTranslation();

  const isMemoryDb = formData.host === ':memory:';

  async function handleBrowse() {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: 'SQLite Database',
            extensions: ['db', 'sqlite', 'sqlite3', 's3db'],
          },
          {
            name: 'All Files',
            extensions: ['*'],
          },
        ],
      });

      if (selected && typeof selected === 'string') {
        onChange('host', selected);
      }
    } catch (err) {
      console.error('Failed to open file dialog:', err);
    }
  }

  async function handleCreate() {
    try {
      const selected = await save({
        filters: [
          {
            name: 'SQLite Database',
            extensions: ['db', 'sqlite', 'sqlite3'],
          },
        ],
        defaultPath: 'new_database.db',
      });

      if (selected && typeof selected === 'string') {
        onChange('host', selected);
      }
    } catch (err) {
      console.error('Failed to save file dialog:', err);
    }
  }

  function handleMemoryToggle() {
    if (isMemoryDb) {
      onChange('host', '');
    } else {
      onChange('host', ':memory:');
    }
  }

  return (
    <div className="rounded-md border border-border bg-background p-4 space-y-4">
      <div className="space-y-2">
        <Label className="flex items-center gap-2">
          <File size={14} className="text-muted-foreground" />
          {t('connection.filePath')}
          <span className="text-error">*</span>
        </Label>
        <div className="flex gap-2">
          <Input
            placeholder={t('connection.filePathPlaceholder')}
            value={formData.host}
            readOnly={!isMemoryDb}
            onChange={e => onChange('host', e.target.value)}
            className={cn(isMemoryDb && 'text-muted-foreground italic', 'cursor-default')}
            disabled={isMemoryDb}
          />
          <div className="flex gap-1">
            <Button
              type="button"
              variant="outline"
              size="icon"
              onClick={handleBrowse}
              title={t('connection.browseFile')}
              disabled={isMemoryDb}
            >
              <FolderOpen size={16} />
            </Button>
            <Button
              type="button"
              variant="outline"
              size="icon"
              onClick={handleCreate}
              title={t('connection.createFile')}
              disabled={isMemoryDb}
            >
              <Plus size={16} />
            </Button>
          </div>
        </div>
        <p className="text-xs text-muted-foreground">{t('connection.sqliteHelp')}</p>
      </div>

      <div className="flex items-center justify-between rounded-md border border-border bg-muted/30 px-3 py-2">
        <div className="flex items-center gap-2">
          <Database size={14} className="text-muted-foreground" />
          <span className="text-sm">{t('connection.inMemory')}</span>
        </div>
        <Button
          type="button"
          variant={isMemoryDb ? 'default' : 'outline'}
          size="sm"
          onClick={handleMemoryToggle}
          className="h-7 text-xs"
        >
          {isMemoryDb ? t('common.enabled') : t('common.disabled')}
        </Button>
      </div>
    </div>
  );
}
