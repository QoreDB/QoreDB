// SPDX-License-Identifier: BUSL-1.1

import { describe, expect, it } from 'vitest';
import { resolveAvailableLocalModel } from './ai';

const models = [
  { id: 'qwen2.5-coder:3b', label: 'qwen2.5-coder:3b' },
  { id: 'qwen3:8b', label: 'qwen3:8b' },
  { id: 'qwen3:latest', label: 'qwen3:latest' },
];

describe('resolveAvailableLocalModel', () => {
  it('keeps an exact installed tag', () => {
    expect(resolveAvailableLocalModel('qwen3:8b', models)).toBe('qwen3:8b');
  });

  it('migrates an untagged alias to the installed latest tag', () => {
    expect(resolveAvailableLocalModel('qwen3', models)).toBe('qwen3:latest');
  });

  it('falls back to an actually installed model when the preference is stale', () => {
    expect(resolveAvailableLocalModel('removed-model', models)).toBe('qwen2.5-coder:3b');
  });

  it('returns undefined when Ollama has no installed models', () => {
    expect(resolveAvailableLocalModel('qwen3', [])).toBeUndefined();
  });
});
