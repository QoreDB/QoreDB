// SPDX-License-Identifier: BUSL-1.1

import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import '/src/i18n';
import '/src/index.css';
import { ReplayReportView } from '/src/components/Replay/ReplayReportView';
import { summarizeVerdicts } from '/src/lib/replay';

const entry = (id, verdict) => ({
  entry_id: id, order: Number(id), query_preview: `SELECT ${id}`,
  verdict, success: verdict === 'match', skip_code: verdict === 'skipped' ? 'cancelled' : null,
  execution_time_ms: 2, expected_execution_time_ms: 2, row_count: verdict === 'match' ? 1 : null, expected_row_count: 1,
  digest: null, expected_digest: null, captured: false, partial_comparison: false,
});
function Fixture() {
  const [cancelled, setCancelled] = useState(true);
  const results = [entry('1', 'match'), entry('2', cancelled ? 'skipped' : 'match')];
  const report = { run: { cancelled }, results, summary: summarizeVerdicts(results) };
  return <main className="p-4"><button type="button" onClick={() => setCancelled(false)}>Complete</button>
    <ReplayReportView report={report} />
  </main>;
}
createRoot(document.getElementById('root')).render(<Fixture />);
