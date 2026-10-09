// SPDX-License-Identifier: Apache-2.0

import { open as openDialog, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import {
  Download,
  Folder,
  FolderPlus,
  Play,
  RefreshCw,
  Star,
  Trash2,
  Upload,
  X,
} from 'lucide-react';
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { UpgradePrompt } from '@/components/License/UpgradePrompt';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tooltip } from '@/components/ui/tooltip';
import { extractVariableReferences } from '@/lib/notebook/notebookVariables';
import {
  createFolder,
  deleteFolder,
  deleteItem,
  exportLibrary,
  importLibrary,
  listFolders,
  listItems,
  type QueryFolder,
  type QueryLibraryExportV1,
  type QueryLibraryItem,
  subscribeQueryLibrary,
  syncWorkspaceLibrary,
  updateItem,
} from '@/lib/query/queryLibrary';
import { confirmDialog } from '@/lib/stores/confirmStore';
import { getWorkspaceState, useWorkspaceStore } from '@/lib/stores/workspaceStore';
import { cn } from '@/lib/utils';
import { useLicense } from '@/providers/LicenseProvider';

const QueryVariablesPrompt = lazy(() =>
  import('./QueryVariablesPrompt').then(module => ({ default: module.QueryVariablesPrompt }))
);

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

export function QueryLibraryModal(props: QueryLibraryModalProps) {
  const projectId = useWorkspaceStore(state => state.projectId);
  return <WorkspaceQueryLibraryModal key={projectId} {...props} projectId={projectId} />;
}

function WorkspaceQueryLibraryModal({
  isOpen,
  onClose,
  onSelectQuery,
  projectId,
}: QueryLibraryModalProps & { projectId: string }) {
  const scope = useRef(0);
  useEffect(() => {
    if (!isOpen) scope.current++;
    return () => {
      scope.current++;
    };
  }, [isOpen]);

  function isCurrent(generation = scope.current) {
    const workspace = getWorkspaceState();
    return (
      generation === scope.current && workspace.projectId === projectId && !workspace.isLoading
    );
  }

  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const [readError, setReadError] = useState(false);
  const [folders, setFolders] = useState<QueryFolder[]>([]);
  const [items, setItems] = useState<QueryLibraryItem[]>([]);
  const [folderFilter, setFolderFilter] = useState<string>('__all__');
  const [search, setSearch] = useState('');
  const [tag, setTag] = useState('');
  const [favoritesOnly, setFavoritesOnly] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');
  const [redactOnExport, setRedactOnExport] = useState(true);
  const [varPromptItem, setVarPromptItem] = useState<QueryLibraryItem | null>(null);

  const folderById = useMemo(() => {
    const map = new Map<string, QueryFolder>();
    for (const folder of folders) map.set(folder.id, folder);
    return map;
  }, [folders]);

  const listOptions = useMemo(() => {
    const folderIdOption =
      folderFilter === '__all__' ? undefined : folderFilter === '__none__' ? null : folderFilter;

    return {
      folderId: folderIdOption,
      search,
      tag: tag.trim() || undefined,
      favoritesOnly,
    };
  }, [favoritesOnly, folderFilter, search, tag]);

  const reload = useCallback(() => {
    try {
      setFolders(listFolders());
      setItems(listItems(listOptions));
      setReadError(false);
    } catch {
      setFolders([]);
      setItems([]);
      setReadError(true);
    }
  }, [listOptions]);

  useEffect(() => {
    if (!isOpen) return;
    reload();
    return subscribeQueryLibrary(reload);
  }, [isOpen, reload]);

  async function refresh() {
    try {
      await syncWorkspaceLibrary();
    } catch {
      toast.error(t('library.updateError'));
    }
    reload();
  }

  function handleCreateFolder() {
    if (!isCurrent()) return;
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

  async function handleDeleteFolder() {
    const generation = scope.current;
    if (!isCurrent(generation)) return;
    if (folderFilter === '__all__' || folderFilter === '__none__') return;
    const folderName = folderById.get(folderFilter)?.name ?? '';
    if (
      !(await confirmDialog({
        description: t('library.deleteFolderConfirm', { name: folderName }),
      }))
    )
      return;
    try {
      if (!isCurrent(generation)) return;
      deleteFolder(folderFilter);
      setFolderFilter('__all__');
      reload();
    } catch {
      toast.error(t('library.updateError'));
    }
  }

  async function handleExport() {
    const generation = scope.current;
    if (!isCurrent(generation)) return;
    try {
      const payload = exportLibrary({ redact: redactOnExport });
      const filePath = await save({
        defaultPath: 'qoredb-query-library.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath || !isCurrent(generation)) return;
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
    const generation = scope.current;
    if (!isCurrent(generation)) return;
    try {
      const filePath = await openDialog({
        multiple: false,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath || Array.isArray(filePath) || !isCurrent(generation)) return;
      const raw = await readTextFile(filePath);
      if (!isCurrent(generation)) return;
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
    if (!isCurrent()) return;
    try {
      updateItem(item.id, { isFavorite: !item.isFavorite });
      reload();
    } catch (err) {
      toast.error(t('library.updateError'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    }
  }

  async function handleDeleteItem(item: QueryLibraryItem) {
    const generation = scope.current;
    if (!isCurrent(generation)) return;
    if (
      !(await confirmDialog({ description: t('library.deleteItemConfirm', { title: item.title }) }))
    )
      return;
    try {
      if (!isCurrent(generation)) return;
      deleteItem(item.id);
      reload();
    } catch {
      toast.error(t('library.updateError'));
    }
  }

  function handleUseItem(item: QueryLibraryItem) {
    if (!isCurrent()) return;
    if (extractVariableReferences(item.query).length > 0) {
      setVarPromptItem(item);
      return;
    }
    onSelectQuery(item.query);
    onClose();
  }

  if (!isOpen) return null;

  if (!isFeatureEnabled('query_library_advanced')) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
        <button
          type="button"
          aria-label={t('common.close')}
          className="absolute inset-0"
          onMouseDown={onClose}
        />
        <div className="relative z-10 w-full max-w-md bg-background border border-border rounded-lg shadow-xl p-6">
          <UpgradePrompt feature="query_library_advanced" />
        </div>
      </div>
    );
  }

  return (
    <>
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
        <button
          type="button"
          aria-label={t('common.close')}
          className="absolute inset-0"
          onMouseDown={onClose}
        />
        <div className="relative z-10 w-full max-w-3xl max-h-[85vh] bg-background border border-border rounded-lg shadow-xl flex flex-col overflow-hidden">
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
              <Label
                htmlFor="ql-redact-export"
                className="text-xs text-muted-foreground select-none"
              >
                {t('library.redactExport')}
              </Label>
            </div>

            <Tooltip content={t('library.refresh')}>
              <Button
                variant="ghost"
                size="icon"
                onClick={refresh}
                className="h-8 w-8"
                aria-label={t('library.refresh')}
              >
                <RefreshCw size={14} />
              </Button>
            </Tooltip>

            <Tooltip content={t('library.import')}>
              <Button
                variant="ghost"
                size="icon"
                onClick={handleImport}
                disabled={readError}
                className="h-8 w-8"
                aria-label={t('library.import')}
              >
                <Upload size={14} />
              </Button>
            </Tooltip>
            <Tooltip content={t('library.export')}>
              <Button
                variant="ghost"
                size="icon"
                onClick={handleExport}
                disabled={readError}
                className="h-8 w-8"
                aria-label={t('library.export')}
              >
                <Download size={14} />
              </Button>
            </Tooltip>
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
              disabled={readError || !newFolderName.trim()}
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
            {readError ? (
              <div role="alert" className="p-4 text-sm text-error">
                {t('library.invalidData')}
              </div>
            ) : items.length === 0 ? (
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
                      type="button"
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
                              type="button"
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

                    <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        onClick={() => handleUseItem(item)}
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
      {varPromptItem && (
        <Suspense fallback={null}>
          <QueryVariablesPrompt
            open={!!varPromptItem}
            onOpenChange={open => {
              if (!open) setVarPromptItem(null);
            }}
            title={varPromptItem.title}
            query={varPromptItem.query}
            variables={varPromptItem.variables}
            onSubmit={resolved => {
              if (!isCurrent()) return;
              onSelectQuery(resolved);
              setVarPromptItem(null);
              onClose();
            }}
          />
        </Suspense>
      )}
    </>
  );
}
