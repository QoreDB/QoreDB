// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import '../../../src/i18n';
import { QueryLibraryModal } from '../../../src/components/Query/QueryLibraryModal';
import { SaveQueryDialog } from '../../../src/components/Query/SaveQueryDialog';
import { GlobalSearch } from '../../../src/components/Search/GlobalSearch';
import { TooltipProvider } from '../../../src/components/ui/tooltip';
import * as library from '../../../src/lib/query/queryLibrary';
import { resolveConfirm, useConfirmState } from '../../../src/lib/stores/confirmStore';
import { useWorkspace, WorkspaceProvider } from '../../../src/providers/WorkspaceProvider';

const emptyCommands = [];

function Probe() {
  const workspace = useWorkspace();
  const [open, setOpen] = useState(false);
  const [saveOpen, setSaveOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const confirm = useConfirmState();
  useEffect(() => {
    window.__confirm = confirm;
    window.__resolveConfirm = resolveConfirm;
  }, [confirm]);
  useEffect(() => {
    window.__workspace = workspace;
    window.__library = library;
    window.__dialogs = { library: setOpen, save: setSaveOpen, search: setSearchOpen };
  }, [workspace]);
  return (
    <TooltipProvider>
      <p>{workspace.projectId}</p>
      <GlobalSearch
        isOpen={searchOpen}
        onClose={() => setSearchOpen(false)}
        commands={emptyCommands}
        features={emptyCommands}
      />
      <QueryLibraryModal
        isOpen={open}
        onClose={() => setOpen(false)}
        onSelectQuery={query => {
          window.__selectedQuery = query;
        }}
      />
      <SaveQueryDialog
        open={saveOpen}
        onOpenChange={setSaveOpen}
        initialQuery="SELECT 1"
        defaultTitle="Draft from A"
      />
    </TooltipProvider>
  );
}

createRoot(document.getElementById('root')).render(
  <WorkspaceProvider>
    <Probe />
  </WorkspaceProvider>
);
