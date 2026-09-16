// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { DATA_MODEL_ORDER, DRIVERS, Driver } from './drivers';

describe('driver picker order', () => {
  it('lists drivers grouped by data model, in the order of the filter chips', () => {
    const models = Object.values(DRIVERS).map(meta => meta.dataModel);
    const groups = models.filter((m, i) => i === 0 || models[i - 1] !== m);

    expect(groups).toEqual(DATA_MODEL_ORDER.filter(m => models.includes(m)));
  });

  it('opens with the mainstream engines', () => {
    const head = Object.values(DRIVERS)
      .slice(0, 4)
      .map(meta => meta.id);

    expect(head).toEqual([Driver.Postgres, Driver.Mysql, Driver.Sqlite, Driver.SqlServer]);
  });
});
