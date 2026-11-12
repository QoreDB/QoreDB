// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import type { TableColumn } from '@/lib/tauri';
import { buildMappingTree } from './searchMappingTree';

const column = (name: string, data_type: string, is_primary_key = false): TableColumn => ({
  name,
  data_type,
  nullable: !is_primary_key,
  is_primary_key,
});

describe('buildMappingTree', () => {
  it('nests object properties and multi-fields under their parent', () => {
    const tree = buildMappingTree([
      column('_id', '_id', true),
      column('title', 'text'),
      column('title.keyword', 'keyword'),
      column('author', 'object'),
      column('author.name', 'text'),
      column('author.name.raw', 'keyword'),
      column('comments', 'nested'),
      column('comments.body', 'text'),
    ]);

    expect(tree.map(n => n.name)).toEqual(['title', 'author', 'comments']);
    const [title, author, comments] = tree;
    expect(title.children).toEqual([
      { name: 'keyword', path: 'title.keyword', type: 'keyword', kind: 'subfield', children: [] },
    ]);
    expect(author.children[0]).toMatchObject({ name: 'name', kind: 'property', type: 'text' });
    expect(author.children[0].children[0]).toMatchObject({
      name: 'raw',
      path: 'author.name.raw',
      kind: 'subfield',
    });
    expect(comments.children[0]).toMatchObject({ name: 'body', kind: 'property' });
  });

  it('keeps an orphan dotted field at the root with its full name', () => {
    const tree = buildMappingTree([column('meta.version', 'long')]);
    expect(tree).toEqual([
      { name: 'meta.version', path: 'meta.version', type: 'long', kind: 'property', children: [] },
    ]);
  });
});
