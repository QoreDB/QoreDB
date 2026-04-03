// SPDX-License-Identifier: Apache-2.0

import { FolderOpen } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useWorkspace } from '@/providers/WorkspaceProvider';

interface CreateWorkspaceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function CreateWorkspaceDialog({ open, onOpenChange }: CreateWorkspaceDialogProps) {
  const { t } = useTranslation();
  const { createWorkspace } = useWorkspace();

  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [isCreating, setIsCreating] = useState(false);

  async function handleSelectPath() {
    const selected = await openDialog({
      directory: true,
      title: t('workspace.path'),
    });
    if (selected) {
      setPath(selected);
      if (!name) {
        const parts = selected.split(/[/\\]/);
        setName(parts[parts.length - 1] || '');
      }
    }
  }

  async function handleCreate() {
    if (!name.trim() || !path.trim()) return;
    setIsCreating(true);
    try {
      const success = await createWorkspace(path, name.trim());
      if (success) {
        onOpenChange(false);
        setName('');
        setPath('');
      }
    } finally {
      setIsCreating(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t('workspace.create')}</DialogTitle>
          <DialogDescription>{t('workspace.createDescription')}</DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div className="space-y-2">
            <Label htmlFor="ws-name">{t('workspace.name')}</Label>
            <Input
              id="ws-name"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="My Project"
              autoFocus
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="ws-path">{t('workspace.path')}</Label>
            <div className="flex gap-2">
              <Input
                id="ws-path"
                value={path}
                onChange={e => setPath(e.target.value)}
                placeholder="/path/to/project"
                className="flex-1"
                readOnly
              />
              <Button variant="outline" size="icon" onClick={handleSelectPath}>
                <FolderOpen size={16} />
              </Button>
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button onClick={handleCreate} disabled={!name.trim() || !path.trim() || isCreating}>
            {isCreating ? t('common.loading') : t('workspace.initialize')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
