// SPDX-License-Identifier: Apache-2.0

import { Eye, Loader2, Puzzle } from 'lucide-react';
import { type ReactNode, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  type CellContribution,
  type CellHookKind,
  type CellOutput,
  runPluginCellHook,
} from '@/lib/plugins';
import type { Value } from '@/lib/tauri';
import { PluginCellOutputView } from './PluginCellOutputView';

interface PluginCellMenuProps {
  actions: CellContribution[];
  renderers: CellContribution[];
  value: Value;
  columnName: string;
  dataType?: string;
  children: ReactNode;
}

type RunState =
  | { status: 'loading' }
  | { status: 'ok'; output: CellOutput }
  | { status: 'error'; message: string };

interface ActiveRun {
  contribution: CellContribution;
  state: RunState;
}

export function PluginCellMenu({
  actions,
  renderers,
  value,
  columnName,
  dataType,
  children,
}: PluginCellMenuProps) {
  const { t } = useTranslation();
  const [active, setActive] = useState<ActiveRun | null>(null);
  const runSeq = useRef(0);

  const run = async (contribution: CellContribution, kind: CellHookKind) => {
    const seq = ++runSeq.current;
    setActive({ contribution, state: { status: 'loading' } });
    try {
      const output = await runPluginCellHook(contribution.id, kind, value, {
        name: columnName,
        dataType,
      });
      if (seq === runSeq.current) setActive({ contribution, state: { status: 'ok', output } });
    } catch (error) {
      if (seq !== runSeq.current) return;
      const message = error instanceof Error ? error.message : String(error);
      setActive({ contribution, state: { status: 'error', message } });
    }
  };

  const close = () => {
    runSeq.current++;
    setActive(null);
  };

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className="block">{children}</div>
        </ContextMenuTrigger>
        <ContextMenuContent className="max-w-72">
          {renderers.length > 0 && (
            <>
              <ContextMenuLabel className="text-xs text-muted-foreground">
                {t('plugins.cell.previewWith')}
              </ContextMenuLabel>
              {renderers.map(renderer => (
                <ContextMenuItem key={renderer.id} onSelect={() => void run(renderer, 'render')}>
                  <Eye />
                  <span className="truncate">{renderer.label}</span>
                </ContextMenuItem>
              ))}
            </>
          )}
          {renderers.length > 0 && actions.length > 0 && <ContextMenuSeparator />}
          {actions.length > 0 && (
            <>
              <ContextMenuLabel className="text-xs text-muted-foreground">
                {t('plugins.cell.actions')}
              </ContextMenuLabel>
              {actions.map(action => (
                <ContextMenuItem
                  key={action.id}
                  title={action.description}
                  onSelect={() => void run(action, 'action')}
                >
                  <Puzzle />
                  <span className="truncate">{action.label}</span>
                </ContextMenuItem>
              ))}
            </>
          )}
        </ContextMenuContent>
      </ContextMenu>

      <Dialog open={active !== null} onOpenChange={open => !open && close()}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{active?.contribution.label}</DialogTitle>
            <DialogDescription>
              {t('plugins.cell.resultFor', { column: columnName })}
            </DialogDescription>
          </DialogHeader>
          <div aria-live="polite" aria-busy={active?.state.status === 'loading'}>
            {active?.state.status === 'loading' && (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                {t('plugins.cell.running')}
              </div>
            )}
            {active?.state.status === 'error' && (
              <p role="alert" className="text-sm text-error">
                {t('plugins.cell.failed', { message: active.state.message })}
              </p>
            )}
            {active?.state.status === 'ok' && <PluginCellOutputView output={active.state.output} />}
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
