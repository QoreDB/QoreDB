// SPDX-License-Identifier: BUSL-1.1

import { describe, expect, it } from 'vitest';
import { Driver } from '@/lib/connection/drivers';
import type { PlanNode } from './explainPlanParser';
import {
  suggestIndexesFromPlan,
  suggestIndexesFromSqlQuery,
  supportsIndexSuggestions,
} from './indexSuggestions';

describe('indexSuggestions', () => {
  it('derives CREATE INDEX from a costly PostgreSQL Seq Scan node', () => {
    const plan: PlanNode = {
      id: 'node-1',
      nodeType: 'Seq Scan',
      relation: 'orders',
      filter: "((status = 'pending'::text) AND (customer_id = 42))",
      totalCost: 420.5,
      planRows: 5000,
      children: [],
      extra: {},
    };

    const suggestions = suggestIndexesFromPlan(plan, Driver.Postgres);
    expect(suggestions).toHaveLength(1);
    expect(suggestions[0].table).toBe('orders');
    expect(suggestions[0].columns).toEqual(['status', 'customer_id']);
    expect(suggestions[0].sql).toBe(
      'CREATE INDEX "idx_orders_status_customer_id" ON "orders" ("status", "customer_id");'
    );
  });

  it('derives CREATE INDEX from a raw SQL query with WHERE and ORDER BY', () => {
    const suggestions = suggestIndexesFromSqlQuery(
      "SELECT id, total FROM public.invoices WHERE tenant_id = ? AND status = 'open' ORDER BY created_at DESC LIMIT 50",
      Driver.Postgres
    );

    expect(suggestions).toHaveLength(1);
    expect(suggestions[0].table).toBe('invoices');
    expect(suggestions[0].columns).toEqual(['tenant_id', 'status', 'created_at']);
    expect(suggestions[0].sql).toBe(
      'CREATE INDEX "idx_invoices_tenant_id_status_created_at" ON "invoices" ("tenant_id", "status", "created_at");'
    );
  });

  it('quotes identifiers using the target SQL dialect', () => {
    const suggestions = suggestIndexesFromSqlQuery(
      'SELECT * FROM `users` WHERE email = ? AND active = 1',
      Driver.Mysql
    );
    expect(suggestions).toHaveLength(1);
    expect(suggestions[0].sql).toBe(
      'CREATE INDEX `idx_users_email_active` ON `users` (`email`, `active`);'
    );
    expect(supportsIndexSuggestions(Driver.Mysql)).toBe(true);
    expect(supportsIndexSuggestions(Driver.Sqlite)).toBe(true);
  });
});
