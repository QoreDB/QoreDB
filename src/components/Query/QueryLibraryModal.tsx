import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { save, open as openDialog } from '@tauri-apps/plugin-dialog';
import { writeTextFile, readTextFile } from '@tauri-apps/plugin-fs';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { cn } from '@/lib/utils';

import {
  createFolder,
  deleteItem,
  deleteFolder,
  exportLibrary,
  importLibrary,
  listFolders,
  listItems,
  updateItem,
  type QueryFolder,
  type QueryLibraryItem,
  type QueryLibraryExportV1,
} from '@/lib/queryLibrary';
import { Download, FolderPlus, Star, Trash2, Upload, X, Play, Folder, RefreshCw } from 'lucide-react';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';

interface QueryLibraryModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSelectQuery: (query: string) => void;
}

function formatTime(timestamp: number): string {
  const date = new Date(timestamp);
  const now = new Date();
  const diffMs = now.getTime() - timestamp;
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffMins < 1) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  if (diffDays < 7) return `${diffDays}d ago`;
  return date.toLocaleDateString();
}

export function QueryLibraryModal({ isOpen, onClose, onSelectQuery }: QueryLibraryModalProps) {
  const { t } = useTranslation();
  const [folders, setFolders] = useState<QueryFolder[]>([]);
  const [items, setItems] = useState<QueryLibraryItem[]>([]);
  const [folderFilter, setFolderFilter] = useState<string>('__all__');
  const [search, setSearch] = useState('');
  const [tag, setTag] = useState('');
  const [favoritesOnly, setFavoritesOnly] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');
  const [redactOnExport, setRedactOnExport] = useState(true);

  const folderById = useMemo(() => {
    const map = new Map<string, QueryFolder>();
    for (const folder of folders) map.set(folder.id, folder);
    return map;
  }, [folders]);

  const listOptions = useMemo(() => {
    const folderIdOption =
      folderFilter === '__all__'
        ? undefined
        : folderFilter === '__none__'
          ? null
          : folderFilter;

    return {
      folderId: folderIdOption,
      search,
      tag: tag.trim() || undefined,
      favoritesOnly,
    };
  }, [favoritesOnly, folderFilter, search, tag]);

  const reload = useCallback(() => {
    setFolders(listFolders());
    setItems(listItems(listOptions));
  }, [listOptions]);

  useEffect(() => {
    if (!isOpen) return;
    reload();
  }, [isOpen, reload]);

  useEffect(() => {
    if (!isOpen) return;
    setItems(listItems(listOptions));
  }, [isOpen, listOptions]);

  function handleCreateFolder() {
    try {
      const created = createFolder(newFolderName);
      setNewFolderName('');
      setFolderFilter(created.id);
      reload();
      toast.success(t('library.folderCreated', { name: created.name }));
    } catch (err) {
      toast.error(t('library.folderCreateError'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    }
  }

  function handleDeleteFolder() {
    if (folderFilter === '__all__' || folderFilter === '__none__') return;
    const folderName = folderById.get(folderFilter)?.name ?? '';
    if (!confirm(t('library.deleteFolderConfirm', { name: folderName }))) return;
    deleteFolder(folderFilter);
    setFolderFilter('__all__');
    reload();
  }

  async function handleExport() {
    try {
      const payload = exportLibrary({ redact: redactOnExport });
      const filePath = await save({
        defaultPath: 'qoredb-query-library.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath) return;
      await writeTextFile(filePath, JSON.stringify(payload, null, 2));
      const name = filePath.split(/[\\/]/).pop() || filePath;
      toast.success(t('library.exportSuccess', { name }));
    } catch (err) {
      toast.error(t('library.exportError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  }

  async function handleImport() {
    try {
      const filePath = await openDialog({
        multiple: false,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath || Array.isArray(filePath)) return;
      const raw = await readTextFile(filePath);
      const parsed = JSON.parse(raw) as QueryLibraryExportV1;
      const result = importLibrary(parsed);
      reload();
      toast.success(
        t('library.importSuccess', {
          folders: result.foldersImported,
          items: result.itemsImported,
        })
      );
    } catch (err) {
      toast.error(t('library.importError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  }

  function handleToggleFavorite(item: QueryLibraryItem) {
    try {
      updateItem(item.id, { isFavorite: !item.isFavorite });
      reload();
    } catch (err) {
      toast.error(t('library.updateError'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    }
  }

  function handleDeleteItem(item: QueryLibraryItem) {
    if (!confirm(t('library.deleteItemConfirm', { title: item.title }))) return;
    deleteItem(item.id);
    reload();
  }

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      onMouseDown={e => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="w-full max-w-3xl max-h-[85vh] bg-background border border-border rounded-lg shadow-xl flex flex-col overflow-hidden">
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <div className="flex items-center gap-2">
            <Folder size={18} className="text-accent" />
            <h2 className="font-semibold">{t('library.title')}</h2>
            <span className="text-xs font-normal text-muted-foreground bg-muted px-1.5 py-0.5 rounded ml-2">
              {items.length}
            </span>
          </div>
          <Button variant="ghost" size="icon" onClick={onClose} className="h-8 w-8">
            <X size={16} />
          </Button>
        </div>

        <div className="flex items-center gap-2 px-4 py-2 border-b border-border bg-muted/20">
          <Select value={folderFilter} onValueChange={value => setFolderFilter(value)}>
            <SelectTrigger className="w-48 h-8">
              <SelectValue placeholder={t('library.folder.all')} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="__all__">{t('library.folder.all')}</SelectItem>
              <SelectItem value="__none__">{t('library.folder.none')}</SelectItem>
              {folders.map(folder => (
                <SelectItem key={folder.id} value={folder.id}>
                  {folder.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Input
            value={search}
            onChange={e => setSearch(e.target.value)}
            placeholder={t('library.searchPlaceholder')}
            className="h-8"
          />

          <Input
            value={tag}
            onChange={e => setTag(e.target.value)}
            placeholder={t('library.tagPlaceholder')}
            className="h-8 w-40"
          />

          <div className="flex items-center gap-2">
            <Checkbox
              id="ql-fav-only"
              checked={favoritesOnly}
              onCheckedChange={checked => setFavoritesOnly(Boolean(checked))}
            />
            <Label htmlFor="ql-fav-only" className="text-xs text-muted-foreground select-none">
              {t('library.favoritesOnly')}
            </Label>
          </div>

          <div className="flex-1" />

          <div className="flex items-center gap-2">
            <Checkbox
              id="ql-redact-export"
              checked={redactOnExport}
              onCheckedChange={checked => setRedactOnExport(Boolean(checked))}
            />
            <Label htmlFor="ql-redact-export" className="text-xs text-muted-foreground select-none">
              {t('library.redactExport')}
            </Label>
          </div>

          <Button variant="ghost" size="icon" onClick={reload} className="h-8 w-8" title={t('library.refresh')}>
            <RefreshCw size={14} />
          </Button>

          <Button variant="ghost" size="icon" onClick={handleImport} className="h-8 w-8" title={t('library.import')}>
            <Upload size={14} />
          </Button>
          <Button variant="ghost" size="icon" onClick={handleExport} className="h-8 w-8" title={t('library.export')}>
            <Download size={14} />
          </Button>
        </div>

        <div className="flex items-center gap-2 px-4 py-2 border-b border-border">
          <Input
            value={newFolderName}
            onChange={e => setNewFolderName(e.target.value)}
            placeholder={t('library.newFolderPlaceholder')}
            className="h-8 w-64"
          />
          <Button
            variant="outline"
            size="sm"
            onClick={handleCreateFolder}
            disabled={!newFolderName.trim()}
            className="h-8"
          >
            <FolderPlus size={14} className="mr-1" />
            {t('library.createFolder')}
          </Button>

          <div className="flex-1" />

          <Button
            variant="ghost"
            size="sm"
            onClick={handleDeleteFolder}
            disabled={folderFilter === '__all__' || folderFilter === '__none__'}
            className={cn(
              'h-8 text-xs text-muted-foreground hover:text-error',
              (folderFilter === '__all__' || folderFilter === '__none__') && 'opacity-50'
            )}
            title={t('library.deleteFolder')}
          >
            <Trash2 size={14} className="mr-1" />
            {t('library.deleteFolder')}
          </Button>
        </div>

        <div className="flex-1 overflow-auto">
          {items.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-48 text-muted-foreground">
              <Folder size={32} className="mb-2 opacity-50" />
              <p className="text-sm">{t('library.empty')}</p>
            </div>
          ) : (
            <div className="divide-y divide-border">
              {items.map(item => (
                <div
                  key={item.id}
                  className="group flex items-start gap-3 px-4 py-3 hover:bg-muted/30 transition-colors"
                >
                  <button
                    className={cn(
                      'mt-1 h-7 w-7 rounded-md flex items-center justify-center transition-colors',
                      item.isFavorite
                        ? 'text-yellow-500 hover:bg-muted'
                        : 'text-muted-foreground hover:text-foreground hover:bg-muted'
                    )}
                    onClick={() => handleToggleFavorite(item)}
                    title={t('library.toggleFavorite')}
                  >
                    <Star size={14} className={item.isFavorite ? 'fill-current' : ''} />
                  </button>

                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <div className="font-medium truncate">{item.title}</div>
                      <div className="text-xs text-muted-foreground">
                        {formatTime(item.updatedAt)}
                      </div>
                      {item.folderId ? (
                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-muted border border-border text-muted-foreground">
                          {folderById.get(item.folderId)?.name ?? t('library.folder.unknown')}
                        </span>
                      ) : null}
                    </div>
                    <pre className="mt-1 font-mono text-xs text-muted-foreground whitespace-pre-wrap break-all line-clamp-3">
                      {item.query}
                    </pre>
                    {item.tags.length > 0 && (
                      <div className="mt-2 flex flex-wrap gap-1">
                        {item.tags.map(tagValue => (
                          <button
                            key={tagValue}
                            className="text-[11px] px-2 py-0.5 rounded-full bg-muted text-muted-foreground border border-border hover:text-foreground"
                            onClick={() => setTag(tagValue)}
                            title={t('library.filterByTag')}
                          >
                            {tagValue}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>

                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7"
                      onClick={() => {
                        onSelectQuery(item.query);
                        onClose();
                      }}
                      title={t('library.useQuery')}
                    >
                      <Play size={14} />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 text-muted-foreground hover:text-error"
                      onClick={() => handleDeleteItem(item)}
                      title={t('library.deleteItem')}
                    >
                      <Trash2 size={14} />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
