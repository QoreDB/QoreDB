// SPDX-License-Identifier: BUSL-1.1

import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import '/src/i18n';
import { useDiffSources } from '/src/components/Diff/hooks/useDiffSources';

const connection = { id: 'fixture', database: 'fixture', driver: 'sqlite' };
const namespace = { database: 'fixture', schema: 'public' };
const leftSource = { type: 'query', query: 'SELECT left', label: 'left' };
const rightSource = { type: 'query', query: 'SELECT right', label: 'right' };

function Sources() {
  const sources = useDiffSources({
    activeConnection: connection,
    initialNamespace: namespace,
    initialLeftSource: leftSource,
    initialRightSource: rightSource,
  });
  window.__sources = sources;
  window.__setLimitAndRefresh = limit => {
    sources.setRowLimit(limit);
    void sources.refresh();
  };
  return (
    <output>{JSON.stringify({ left: sources.leftSource, right: sources.rightSource })}</output>
  );
}

function Fixture() {
  const [mounted, setMounted] = useState(true);
  window.__unmount = () => setMounted(false);
  return mounted ? <Sources /> : <p>Closed</p>;
}

createRoot(document.getElementById('root')).render(<Fixture />);
