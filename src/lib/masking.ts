// SPDX-License-Identifier: BUSL-1.1

import { invoke } from '@/lib/transport';
import type { ConnectionMasking, MaskingRule, VaultResponse } from './tauri';

export const EMPTY_MASKING: ConnectionMasking = { rules: [], mask_detected_columns: false };

/** Schema qualifiers are ignored, as in the backend: `public.users` matches `users`. */
function sameTable(a: string, b: string): boolean {
  const last = (name: string) => (name.split('.').pop() ?? name).trim().toLowerCase();
  return last(a) === last(b);
}

function sameRule(a: MaskingRule, b: MaskingRule): boolean {
  return a.table === b.table && a.column === b.column && a.mode === b.mode;
}

/** `table` is undefined for free query results, where rules match by column name. */
export function findRule(
  masking: ConnectionMasking,
  table: string | undefined,
  column: string
): MaskingRule | undefined {
  return masking.rules.find(
    rule =>
      rule.column.toLowerCase() === column.toLowerCase() &&
      (!rule.table || !table || sameTable(rule.table, table))
  );
}

export function withRule(masking: ConnectionMasking, rule: MaskingRule): ConnectionMasking {
  return {
    ...masking,
    rules: [
      ...masking.rules.filter(
        existing =>
          !(
            existing.column.toLowerCase() === rule.column.toLowerCase() &&
            existing.table === rule.table
          )
      ),
      rule,
    ],
  };
}

export function withoutRule(masking: ConnectionMasking, rule: MaskingRule): ConnectionMasking {
  return { ...masking, rules: masking.rules.filter(existing => !sameRule(existing, rule)) };
}

/** True when `next` leaves visible something `previous` masked. */
export function removesMasking(previous: ConnectionMasking, next: ConnectionMasking): boolean {
  const keepsDetection = !previous.mask_detected_columns || next.mask_detected_columns;
  const keepsRules = previous.rules.every(rule => next.rules.some(other => sameRule(rule, other)));
  return !(keepsDetection && keepsRules);
}

export function normalizeMasking(masking: ConnectionMasking): ConnectionMasking {
  return {
    ...masking,
    rules: masking.rules
      .map(rule => ({ ...rule, table: rule.table.trim(), column: rule.column.trim() }))
      .filter(rule => rule.column.length > 0),
  };
}

export async function setConnectionMasking(
  projectId: string,
  connectionId: string,
  masking: ConnectionMasking
): Promise<VaultResponse> {
  return invoke('set_connection_masking', { projectId, connectionId, masking });
}
