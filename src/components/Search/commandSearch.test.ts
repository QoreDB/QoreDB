// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { commandMatchesQuery } from './commandSearch';

const qoreAiCommand = {
  label: 'Open Qore AI',
  keywords: ['qore ai', 'qoreia', 'assistant ia'],
};

describe('commandMatchesQuery', () => {
  it('finds a command through localized aliases', () => {
    expect(commandMatchesQuery(qoreAiCommand, 'qoreia')).toBe(true);
    expect(commandMatchesQuery(qoreAiCommand, 'assistant ia')).toBe(true);
  });

  it('still matches the visible label', () => {
    expect(commandMatchesQuery(qoreAiCommand, 'QORE AI')).toBe(true);
  });

  it('rejects unrelated searches', () => {
    expect(commandMatchesQuery(qoreAiCommand, 'migrations')).toBe(false);
  });
});
