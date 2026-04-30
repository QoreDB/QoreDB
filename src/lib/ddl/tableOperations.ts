// SPDX-License-Identifier: Apache-2.0

import type { Driver } from '../drivers';
import { buildQualifiedTableName } from './identifiers';
import type { NamespaceLike } from './types';

export function buildDropTableSQL(
  namespace: NamespaceLike,
  tableName: string,
  driver: Driver
): string {
  return `DROP TABLE ${buildQualifiedTableName(namespace, tableName, driver)};`;
}

export function buildTruncateTableSQL(
  namespace: NamespaceLike,
  tableName: string,
  driver: Driver
): string {
  return `TRUNCATE TABLE ${buildQualifiedTableName(namespace, tableName, driver)}`;
}
