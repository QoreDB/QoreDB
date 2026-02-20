// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

import {
  addItem,
  createFolder,
  listFolders,
  parseTags,
  type QueryFolder,
} from '@/lib/queryLibrary';

interface SaveQueryDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initialQuery: string;
  driver?: string;
  database?: string;
  defaultTitle?: string;
  defaultFolderId?: string | null;
}

type FolderMode = 'existing' | 'new';

function inferTitleFromQuery(query: string): string {
  const trimmed = query.trim();
  if (!trimmed) return '';
  const firstLine = trimmed.split('\n')[0]?.trim() ?? '';
  return firstLine.slice(0, 64) || 'Query';
}

export function SaveQueryDialog({
  open,
  onOpenChange,
  initialQuery,
  driver,
  database,
  defaultTitle,
  defaultFolderId = null,
}: SaveQueryDialogProps) {
  const { t } = useTranslation();
  const [folders, setFolders] = useState<QueryFolder[]>([]);
  const [title, setTitle] = useState('');
  const [tagsRaw, setTagsRaw] = useState('');
  const [isFavorite, setIsFavorite] = useState(false);
  const [folderMode, setFolderMode] = useState<FolderMode>('existing');
  const [folderId, setFolderId] = useState<string | null>(defaultFolderId);
  const [newFolderName, setNewFolderName] = useState('');

  const parsedTags = useMemo(() => parseTags(tagsRaw), [tagsRaw]);

  useEffect(() => {
    if (!open) return;
    setFolders(listFolders());
    setTitle((defaultTitle ?? inferTitleFromQuery(initialQuery)).trim());
    setTagsRaw('');
    setIsFavorite(false);
    setFolderMode('existing');
    setFolderId(defaultFolderId);
    setNewFolderName('');
  }, [open, defaultFolderId, defaultTitle, initialQuery]);

  function close() {
    onOpenChange(false);
  }

  function resolveFolderId(): string | null {
    if (folderMode === 'new') {
      const created = createFolder(newFolderName);
      return created.id;
    }
    return folderId ?? null;
  }

  function handleSave() {
    try {
      const resolvedFolderId = resolveFolderId();
      addItem({
        title,
        query: initialQuery,
        folderId: resolvedFolderId,
        tags: parsedTags,
        isFavorite,
        driver,
        database,
      });
      toast.success(t('library.saved'));
      close();
    } catch (err) {
      toast.error(t('library.saveError'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('library.saveTitle')}</DialogTitle>
        </DialogHeader>

        <div className="grid gap-4 py-2">
          <div className="grid gap-2">
            <Label htmlFor="ql-title">{t('library.fields.title')}</Label>
            <Input
              id="ql-title"
              value={title}
              onChange={e => setTitle(e.target.value)}
              placeholder={t('library.placeholders.title')}
            />
          </div>

          <div className="grid gap-2">
            <Label>{t('library.fields.folder')}</Label>
            <div className="flex gap-2">
              <Select
                value={folderMode}
                onValueChange={value => setFolderMode(value as FolderMode)}
              >
                <SelectTrigger className="w-40">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="existing">{t('library.folderMode.existing')}</SelectItem>
                  <SelectItem value="new">{t('library.folderMode.new')}</SelectItem>
                </SelectContent>
              </Select>

              {folderMode === 'existing' ? (
                <Select
                  value={folderId ?? '__none__'}
                  onValueChange={value => setFolderId(value === '__none__' ? null : value)}
                >
                  <SelectTrigger className="flex-1">
                    <SelectValue placeholder={t('library.placeholders.folder')} />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="__none__">{t('library.folder.none')}</SelectItem>
                    {folders.map(folder => (
                      <SelectItem key={folder.id} value={folder.id}>
                        {folder.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              ) : (
                <Input
                  className="flex-1"
                  value={newFolderName}
                  onChange={e => setNewFolderName(e.target.value)}
                  placeholder={t('library.placeholders.newFolder')}
                />
              )}
            </div>
          </div>

          <div className="grid gap-2">
            <Label htmlFor="ql-tags">{t('library.fields.tags')}</Label>
            <Input
              id="ql-tags"
              value={tagsRaw}
              onChange={e => setTagsRaw(e.target.value)}
              placeholder={t('library.placeholders.tags')}
            />
            {parsedTags.length > 0 && (
              <div className="flex flex-wrap gap-1">
                {parsedTags.map(tag => (
                  <span
                    key={tag}
                    className="text-[11px] px-2 py-0.5 rounded-full bg-muted text-muted-foreground border border-border"
                  >
                    {tag}
                  </span>
                ))}
              </div>
            )}
          </div>

          <div className="flex items-center gap-2">
            <Checkbox
              id="ql-favorite"
              checked={isFavorite}
              onCheckedChange={checked => setIsFavorite(Boolean(checked))}
            />
            <Label htmlFor="ql-favorite" className="text-sm select-none">
              {t('library.fields.favorite')}
            </Label>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={close}>
            {t('common.cancel')}
          </Button>
          <Button onClick={handleSave} disabled={!title.trim() || !initialQuery.trim()}>
            {t('library.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
