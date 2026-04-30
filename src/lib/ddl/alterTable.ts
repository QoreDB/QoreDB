// SPDX-License-Identifier: Apache-2.0

import type { Driver } from '../drivers';
import type { AlterOp, TableDefinition } from './types';

export function diffTableDefinitions(_before: TableDefinition, _after: TableDefinition): AlterOp[] {
  throw new Error('diffTableDefinitions: not implemented (Phase 3c)');
}

export function buildAlterTableSQL(
  _table: TableDefinition,
  _ops: AlterOp[],
  _driver: Driver
): string[] {
  throw new Error('buildAlterTableSQL: not implemented (Phase 3c)');
}
