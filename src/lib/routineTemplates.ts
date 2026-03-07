// SPDX-License-Identifier: Apache-2.0

import { Driver } from './drivers';
import type { Namespace, RoutineType } from './tauri';

/**
 * Returns a driver-specific SQL template for creating a new routine.
 */
export function getRoutineTemplate(
  driver: Driver,
  routineType: RoutineType,
  namespace: Namespace
): string {
  switch (driver) {
    case Driver.Postgres:
      return routineType === 'Function'
        ? postgresCreateFunction(namespace)
        : postgresCreateProcedure(namespace);
    case Driver.Mysql:
      return routineType === 'Function'
        ? mysqlCreateFunction(namespace)
        : mysqlCreateProcedure(namespace);
    case Driver.SqlServer:
      return routineType === 'Function'
        ? sqlserverCreateFunction(namespace)
        : sqlserverCreateProcedure(namespace);
    default:
      return '-- Routines are not supported for this driver\n';
  }
}

function postgresCreateFunction(ns: Namespace): string {
  const schema = ns.schema ?? 'public';
  return `CREATE OR REPLACE FUNCTION "${schema}".my_function(param1 integer, param2 text)
RETURNS text
LANGUAGE plpgsql
AS $$
BEGIN
    -- Your logic here
    RETURN param2;
END;
$$;
`;
}

function postgresCreateProcedure(ns: Namespace): string {
  const schema = ns.schema ?? 'public';
  return `CREATE OR REPLACE PROCEDURE "${schema}".my_procedure(param1 integer)
LANGUAGE plpgsql
AS $$
BEGIN
    -- Your logic here
END;
$$;
`;
}

function mysqlCreateFunction(ns: Namespace): string {
  return `CREATE FUNCTION \`${ns.database}\`.my_function(param1 INT, param2 VARCHAR(255))
RETURNS VARCHAR(255)
DETERMINISTIC
BEGIN
    -- Your logic here
    RETURN param2;
END;
`;
}

function mysqlCreateProcedure(ns: Namespace): string {
  return `CREATE PROCEDURE \`${ns.database}\`.my_procedure(IN param1 INT)
BEGIN
    -- Your logic here
    SELECT param1;
END;
`;
}

function sqlserverCreateFunction(ns: Namespace): string {
  const schema = ns.schema ?? 'dbo';
  return `CREATE FUNCTION [${schema}].[my_function](@param1 INT, @param2 NVARCHAR(255))
RETURNS NVARCHAR(255)
AS
BEGIN
    -- Your logic here
    RETURN @param2;
END;
`;
}

function sqlserverCreateProcedure(ns: Namespace): string {
  const schema = ns.schema ?? 'dbo';
  return `CREATE PROCEDURE [${schema}].[my_procedure]
    @param1 INT
AS
BEGIN
    SET NOCOUNT ON;
    -- Your logic here
    SELECT @param1;
END;
`;
}
