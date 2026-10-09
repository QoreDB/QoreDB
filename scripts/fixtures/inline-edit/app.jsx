// SPDX-License-Identifier: Apache-2.0
import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import '/src/index.css';
import '/src/i18n';
import { DataGrid } from '/src/components/Grid/DataGrid';
import { TooltipProvider } from '/src/components/ui/tooltip';
import { useInfiniteTableData } from '/src/hooks/useInfiniteTableData';

const ns = { database: 'fixture', schema: 'public' };
const pk = ['id'];
const schema = {
  columns: [
    { name: 'id', data_type: 'integer', nullable: false },
    { name: 'name', data_type: 'text', nullable: false },
    { name: 'rank', data_type: 'integer', nullable: false },
    { name: 'bytes', data_type: 'BLOB', nullable: true },
  ],
  primary_key: pk,
  foreign_keys: [],
  indexes: [],
};
function Fixture() {
  const [session, setSession] = useState('fixture-a');
  const [sortColumn, setSortColumn] = useState('id');
  const [sortDirection, setSortDirection] = useState('asc');
  const [searchTerm, setSearchTerm] = useState('');
  const [filters, setFilters] = useState(undefined);
  const data = useInfiniteTableData({
    sessionId: session,
    namespace: ns,
    tableName: 'items',
    primaryKey: pk,
    keysetColumns: pk,
    sortColumn,
    sortDirection,
    searchTerm,
    filters,
  });
  window.__fixture = { setSession, setSortColumn, setSearchTerm, data };
  return (
    <main style={{ padding: 16, height: 620, width: 1100 }}>
      <button type="button" id="more" onClick={() => data.fetchNextChunk()}>
        Load more
      </button>
      <button type="button" id="outside">
        Outside
      </button>
      <output id="loaded">{data.loadedRows}</output>
      <div style={{ height: 550 }}>
        <DataGrid
          result={data.data}
          driver="sqlite"
          sessionId={session}
          namespace={ns}
          tableName="items"
          primaryKey={pk}
          tableSchema={schema}
          onUpdateCell={data.updateCell}
          onRowsUpdated={data.refresh}
          infiniteScrollLoadedRows={data.loadedRows}
          infiniteScrollIsFetchingMore={data.isFetchingMore}
          infiniteScrollIsComplete={data.isComplete}
          infiniteScrollOrderingGuarantee={data.orderingGuarantee}
          onFetchMore={data.fetchNextChunk}
          onServerSortChange={(col, dir) => {
            setSortColumn(col);
            setSortDirection(dir);
          }}
          serverSortColumn={sortColumn}
          serverSortDirection={sortDirection}
          serverSearchTerm={searchTerm}
          onServerSearchChange={setSearchTerm}
          onServerColumnFiltersChange={setFilters}
        />
      </div>
    </main>
  );
}
createRoot(document.getElementById('root')).render(
  <React.StrictMode>
    <TooltipProvider>
      <Fixture />
    </TooltipProvider>
  </React.StrictMode>
);
