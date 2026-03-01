// SPDX-License-Identifier: Apache-2.0

import { Driver } from './drivers';
import type { Namespace } from './tauri';

/**
 * Returns a driver-specific SQL template for creating a new trigger.
 */
export function getTriggerTemplate(
  driver: Driver,
  namespace: Namespace
): string {
  switch (driver) {
    case Driver.Postgres:
      return postgresCreateTrigger(namespace);
    case Driver.Mysql:
      return mysqlCreateTrigger(namespace);
    case Driver.SqlServer:
      return sqlserverCreateTrigger(namespace);
    default:
      return '-- Triggers are not supported for this driver\n';
  }
}

/**
 * Returns a driver-specific SQL template for creating a new MySQL event.
 */
export function getEventTemplate(namespace: Namespace): string {
  return mysqlCreateEvent(namespace);
}

function postgresCreateTrigger(ns: Namespace): string {
  const schema = ns.schema ?? 'public';
  return `-- Step 1: Create trigger function
CREATE OR REPLACE FUNCTION "${schema}".my_trigger_function()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    -- Your logic here
    RETURN NEW;
END;
$$;

-- Step 2: Create trigger
CREATE TRIGGER my_trigger
    BEFORE INSERT ON "${schema}".my_table
    FOR EACH ROW
    EXECUTE FUNCTION "${schema}".my_trigger_function();
`;
}

function mysqlCreateTrigger(ns: Namespace): string {
  return `CREATE TRIGGER \`${ns.database}\`.my_trigger
    BEFORE INSERT ON \`${ns.database}\`.my_table
    FOR EACH ROW
BEGIN
    -- Your logic here
    SET NEW.updated_at = NOW();
END;
`;
}

function sqlserverCreateTrigger(ns: Namespace): string {
  const schema = ns.schema ?? 'dbo';
  return `CREATE TRIGGER [${schema}].[my_trigger]
ON [${schema}].[my_table]
AFTER INSERT
AS
BEGIN
    SET NOCOUNT ON;
    -- Your logic here
END;
`;
}

function mysqlCreateEvent(ns: Namespace): string {
  return `CREATE EVENT \`${ns.database}\`.my_event
ON SCHEDULE EVERY 1 HOUR
DO
BEGIN
    -- Your logic here
END;
`;
}
