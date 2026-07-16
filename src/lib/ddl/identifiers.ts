// SPDX-License-Identifier: Apache-2.0

import { Driver, getDriverMetadata } from '../connection/drivers';
import type { NamespaceLike } from './types';

export function quoteIdentifier(identifier: string, driver: Driver): string {
  const driverMeta = getDriverMetadata(driver);
  const { quoteStart, quoteEnd } = driverMeta.identifier;
  // Only the closing delimiter can end the identifier, so only it needs
  // doubling. `split`/`join` rather than a RegExp: the delimiters are regex
  // metacharacters on SQL Server (`[`, `]`).
  const escaped = identifier.split(quoteEnd).join(quoteEnd + quoteEnd);
  return `${quoteStart}${escaped}${quoteEnd}`;
}

export function buildQualifiedTableName(
  namespace: NamespaceLike,
  tableName: string,
  driver: Driver
): string {
  if (driver === Driver.Sqlite) {
    return quoteIdentifier(tableName, driver);
  }

  const driverMeta = getDriverMetadata(driver);
  const schema = namespace.schema || undefined;
  const database = namespace.database;

  if (driverMeta.identifier.namespaceStrategy === 'schema' && schema) {
    return `${quoteIdentifier(schema, driver)}.${quoteIdentifier(tableName, driver)}`;
  }

  if (driverMeta.identifier.namespaceStrategy === 'database' && database) {
    return `${quoteIdentifier(database, driver)}.${quoteIdentifier(tableName, driver)}`;
  }

  return quoteIdentifier(tableName, driver);
}
