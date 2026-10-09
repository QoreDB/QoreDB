// SPDX-License-Identifier: BUSL-1.1

import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import '/src/i18n';
import '/src/index.css';
import { SchemaDiffViewer } from '/src/components/Migrations/SchemaDiffViewer';

function Fixture() {
  const [mounted, setMounted] = useState(true);
  window.__closeSchema = () => setMounted(false);
  return (
    <main style={{ height: 600 }}>
      {mounted ? (
        <SchemaDiffViewer leftConnectionId="left" rightConnectionId="right" />
      ) : (
        <p>Closed</p>
      )}
    </main>
  );
}
createRoot(document.getElementById('root')).render(<Fixture />);
