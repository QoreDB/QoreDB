// SPDX-License-Identifier: Apache-2.0

import { Driver, getDriverMetadata } from '../connection/drivers';
import type { NamespaceLike } from './types';

export function quoteIdentifier(identifier: string, driver: Driver): string {
  const driverMeta = getDriverMetadata(driver);
  const { quoteStart, quoteEnd } = driverMeta.identifier;
  let escaped = identifier;
  if (quoteStart === quoteEnd) {
    escaped = identifier.replace(new RegExp(quoteStart, 'g'), `${quoteStart}${quoteStart}`);
  } else {
    escaped = identifier
      .replace(new RegExp(quoteStart, 'g'), `${quoteStart}${quoteStart}`)
      .replace(new RegExp(quoteEnd, 'g'), `${quoteEnd}${quoteEnd}`);
  }
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
