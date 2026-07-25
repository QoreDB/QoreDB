// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { Driver } from '@/lib/connection/drivers';
import { buildCreateIndexSQL } from './createTable';

describe('buildCreateIndexSQL', () => {
  it('quotes the identifiers for the driver', () => {
    expect(buildCreateIndexSQL('"public"."users"', 'email', Driver.Postgres)).toContain(
      'ON "public"."users" ("email")'
    );
    expect(buildCreateIndexSQL('`app`.`users`', 'email', Driver.Mysql)).toContain(
      'ON `app`.`users` (`email`)'
    );
  });

  it('derives a name that survives qualified table references', () => {
    const sql = buildCreateIndexSQL('"public"."users"', 'email', Driver.Postgres);
    expect(sql).toMatch(/CREATE INDEX "idx_public_users_email"/);
  });
});
