// SPDX-License-Identifier: Apache-2.0

import type { TableColumn } from '@/lib/tauri';

export type MappingNodeKind = 'property' | 'subfield';

export interface MappingNode {
  name: string;
  path: string;
  type: string;
  kind: MappingNodeKind;
  children: MappingNode[];
}

const CONTAINER_TYPES = new Set(['object', 'nested']);

/**
 * Rebuilds the mapping hierarchy from the flattened columns `describe_table`
 * returns for search engines: a dotted child of an `object`/`nested` field is
 * a property, a dotted child of a leaf field is a multi-field such as
 * `title.keyword`.
 */
export function buildMappingTree(columns: TableColumn[]): MappingNode[] {
  const roots: MappingNode[] = [];
  const byPath = new Map<string, MappingNode>();

  for (const column of columns) {
    if (column.is_primary_key && column.name === '_id') continue;
    const dot = column.name.lastIndexOf('.');
    const parent = dot > 0 ? byPath.get(column.name.slice(0, dot)) : undefined;
    const node: MappingNode = {
      name: parent ? column.name.slice(dot + 1) : column.name,
      path: column.name,
      type: column.data_type,
      kind: parent && !CONTAINER_TYPES.has(parent.type) ? 'subfield' : 'property',
      children: [],
    };
    byPath.set(column.name, node);
    (parent ? parent.children : roots).push(node);
  }

  return roots;
}
