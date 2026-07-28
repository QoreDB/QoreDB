// SPDX-License-Identifier: Apache-2.0

import type { CrashReport } from '@/lib/tauri';

const ISSUE_URL = 'https://github.com/QoreDB/QoreDB/issues/new';

/**
 * GitHub rejects request URIs past roughly 8 KB, and the prefilled body is only
 * a starting point — the full report stays available via copy / logs folder.
 */
const MAX_BODY_REPORT_CHARS = 4000;

export function buildCrashIssueUrl(report: CrashReport, whatHappenedLabel: string): string {
  const excerpt =
    report.content.length > MAX_BODY_REPORT_CHARS
      ? `${report.content.slice(0, MAX_BODY_REPORT_CHARS)}\n…[truncated]`
      : report.content;

  const body = [
    `## ${whatHappenedLabel}`,
    '',
    '',
    '## Crash report',
    '',
    '```',
    excerpt,
    '```',
  ].join('\n');

  const params = new URLSearchParams({
    title: `Crash: ${report.kind}`,
    body,
    labels: 'crash',
  });

  return `${ISSUE_URL}?${params.toString()}`;
}
