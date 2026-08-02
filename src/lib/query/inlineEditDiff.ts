// SPDX-License-Identifier: BUSL-1.1

export type DiffLineKind = 'context' | 'added' | 'removed';

export interface DiffLine {
  kind: DiffLineKind;
  text: string;
}

/** Beyond this, the quadratic LCS table costs more than the diff is worth. */
const MAX_LINES = 400;

/**
 * Line-level diff between the query before and after an AI rewrite. Removed
 * lines are emitted before the added ones that replace them, so a hunk reads
 * top-down like a unified diff.
 *
 * Falls back to a whole-block replacement above [`MAX_LINES`]; a rewrite that
 * large is not something a reader compares line by line anyway.
 */
export function diffLines(before: string, after: string): DiffLine[] {
  const a = before.split('\n');
  const b = after.split('\n');

  if (a.length > MAX_LINES || b.length > MAX_LINES) {
    return [
      ...a.map((text): DiffLine => ({ kind: 'removed', text })),
      ...b.map((text): DiffLine => ({ kind: 'added', text })),
    ];
  }

  // lcs[i][j] = length of the longest common subsequence of a[i:] and b[j:].
  const lcs: number[][] = Array.from({ length: a.length + 1 }, () =>
    new Array<number>(b.length + 1).fill(0)
  );
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }

  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      out.push({ kind: 'context', text: a[i] });
      i++;
      j++;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      out.push({ kind: 'removed', text: a[i] });
      i++;
    } else {
      out.push({ kind: 'added', text: b[j] });
      j++;
    }
  }
  while (i < a.length) out.push({ kind: 'removed', text: a[i++] });
  while (j < b.length) out.push({ kind: 'added', text: b[j++] });

  return out;
}

export function countChanges(lines: DiffLine[]): { added: number; removed: number } {
  return {
    added: lines.filter(line => line.kind === 'added').length,
    removed: lines.filter(line => line.kind === 'removed').length,
  };
}
