// SPDX-License-Identifier: BUSL-1.1

import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import '/src/index.css';
import '/src/i18n';
import { DiffResultsGrid } from '/src/components/Diff/DiffResultsGrid';
import { DiffSourcePanel } from '/src/components/Diff/DiffSourcePanel';
import { QueryLibraryModal } from '/src/components/Query/QueryLibraryModal';
import { TooltipProvider } from '/src/components/ui/tooltip';
import { compareResults } from '/src/lib/diffUtils';

function result(values) {
  return {
    columns: ['id', 'name'].map(name => ({ name, data_type: 'text', nullable: true })),
    rows: values.map(values => ({ values })),
    execution_time_ms: 0,
  };
}

function Fixture() {
  const [scenario, setScenario] = useState('duplicates');
  const [hideRows, setHideRows] = useState(false);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [selectedQuery, setSelectedQuery] = useState('');
  const [retry, setRetry] = useState('');
  let left = result([
    [1, 'a'],
    [1, 'b'],
  ]);
  let right = result([
    [1, 'b'],
    [1, 'c'],
  ]);
  if (scenario === 'equal' || scenario === 'truncated') {
    left = result([[1, 'same']]);
    right = result([[1, 'same']]);
  }
  const diff = compareResults(left, right, ['id'], { truncated: scenario === 'truncated' });
  return (
    <TooltipProvider>
      <main style={{ padding: 16, height: 640 }}>
        <div className="flex gap-3 mb-3">
          {['duplicates', 'equal', 'truncated', 'retry'].map(value => (
            <button key={value} type="button" onClick={() => setScenario(value)}>
              {value}
            </button>
          ))}
          <button type="button" onClick={() => setHideRows(value => !value)}>
            Toggle rows
          </button>
          <button type="button" onClick={() => setLibraryOpen(true)}>
            Open library
          </button>
        </div>
        <output id="stats">{JSON.stringify(diff.stats)}</output>
        <output id="selected-query">{selectedQuery}</output>
        <output id="retry-result">{retry}</output>
        {scenario === 'retry' && (
          <DiffSourcePanel
            label="Synthetic source"
            connections={[
              { id: 'fixture', name: 'Synthetic connection', environment: 'development' },
            ]}
            source={{
              mode: 'query',
              connectionId: 'fixture',
              connectionError: 'Synthetic connection failure',
              loading: false,
              connecting: false,
              namespacesLoading: false,
            }}
            onConnectionChange={setRetry}
            onNamespaceChange={() => {}}
            onSourceChange={() => {}}
            onExecute={() => setRetry('execute')}
          />
        )}
        <div style={{ height: 500 }}>
          <DiffResultsGrid diffResult={diff} filteredRows={hideRows ? [] : diff.rows} />
        </div>
        <QueryLibraryModal
          isOpen={libraryOpen}
          onClose={() => setLibraryOpen(false)}
          onSelectQuery={setSelectedQuery}
        />
      </main>
    </TooltipProvider>
  );
}

createRoot(document.getElementById('root')).render(<Fixture />);
