// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, ChevronRight, CornerDownRight, Loader2 } from 'lucide-react';
import { type ReactNode, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { TableSchema } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { buildMappingTree, type MappingNode } from './searchMappingTree';

interface SearchIndexTreeItemProps {
  indexName: string;
  loadSchema: () => Promise<TableSchema | null>;
  children: ReactNode;
}

type MappingState =
  | { status: 'idle' | 'loading' }
  | { status: 'ready'; nodes: MappingNode[] }
  | { status: 'error' };

export function SearchIndexTreeItem({ indexName, loadSchema, children }: SearchIndexTreeItemProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [mapping, setMapping] = useState<MappingState>({ status: 'idle' });
  const listId = useId();

  const toggle = () => {
    const next = !expanded;
    setExpanded(next);
    if (next && (mapping.status === 'idle' || mapping.status === 'error')) {
      setMapping({ status: 'loading' });
      loadSchema()
        .then(schema =>
          setMapping(
            schema
              ? { status: 'ready', nodes: buildMappingTree(schema.columns) }
              : { status: 'error' }
          )
        )
        .catch(() => setMapping({ status: 'error' }));
    }
  };

  return (
    <div>
      <div className="relative">
        <button
          type="button"
          className="absolute left-0.5 top-1/2 z-10 -translate-y-1/2 rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
          aria-expanded={expanded}
          aria-controls={listId}
          aria-label={t(expanded ? 'dbtree.mapping.collapse' : 'dbtree.mapping.expand', {
            name: indexName,
          })}
          onClick={toggle}
        >
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </button>
        {children}
      </div>
      {expanded && (
        <div id={listId} className="ml-7 border-l border-border/60 pl-1">
          {mapping.status === 'loading' && (
            <div className="flex items-center gap-1.5 px-2 py-1 text-xs text-muted-foreground">
              <Loader2 size={11} className="animate-spin" />
              {t('common.loading')}
            </div>
          )}
          {mapping.status === 'error' && (
            <div className="px-2 py-1 text-xs text-error">{t('dbtree.mapping.failed')}</div>
          )}
          {mapping.status === 'ready' &&
            (mapping.nodes.length === 0 ? (
              <div className="px-2 py-1 text-xs italic text-muted-foreground">
                {t('dbtree.mapping.empty')}
              </div>
            ) : (
              <ul aria-label={t('dbtree.mapping.label', { name: indexName })}>
                {mapping.nodes.map(node => (
                  <MappingTreeNode key={node.path} node={node} depth={0} />
                ))}
              </ul>
            ))}
        </div>
      )}
    </div>
  );
}

function MappingTreeNode({ node, depth }: { node: MappingNode; depth: number }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(depth === 0 && node.children.length <= 8);
  const hasChildren = node.children.length > 0;
  const isSubfield = node.kind === 'subfield';

  return (
    <li className="list-none">
      <button
        type="button"
        className={cn(
          'flex w-full items-center gap-1 rounded px-1 py-0.5 text-left text-xs hover:bg-muted focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40',
          !hasChildren && 'cursor-default'
        )}
        style={{ paddingLeft: `${depth * 10 + 4}px` }}
        title={node.path}
        aria-expanded={hasChildren ? open : undefined}
        onClick={() => hasChildren && setOpen(value => !value)}
        tabIndex={hasChildren ? 0 : -1}
      >
        <span className="w-3 shrink-0 text-muted-foreground">
          {hasChildren ? (
            open ? (
              <ChevronDown size={11} />
            ) : (
              <ChevronRight size={11} />
            )
          ) : isSubfield ? (
            <CornerDownRight size={11} />
          ) : null}
        </span>
        <span
          className={cn(
            'truncate font-mono',
            isSubfield ? 'text-muted-foreground' : 'text-foreground/90'
          )}
        >
          {node.name}
        </span>
        <span className="ml-auto shrink-0 font-mono text-[10px] text-muted-foreground">
          {isSubfield ? t('dbtree.mapping.subfield', { type: node.type }) : node.type}
        </span>
      </button>
      {hasChildren && open && (
        <ul>
          {node.children.map(child => (
            <MappingTreeNode key={child.path} node={child} depth={depth + 1} />
          ))}
        </ul>
      )}
    </li>
  );
}
