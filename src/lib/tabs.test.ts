// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { createMigrationsTab } from './tabs';

describe('createMigrationsTab', () => {
  it('keeps the active database as the migration target', () => {
    const tab = createMigrationsTab({ database: 'pulse' });

    expect(tab.type).toBe('migrations');
    expect(tab.namespace).toEqual({ database: 'pulse' });
  });
});
