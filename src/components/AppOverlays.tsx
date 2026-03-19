// SPDX-License-Identifier: Apache-2.0

import { memo, useEffect, useState } from 'react';
import { ConnectionModal } from '@/components/Connection/ConnectionModal';
import { OnboardingModal } from '@/components/Onboarding/OnboardingModal';
import { QueryLibraryModal } from '@/components/Query/QueryLibraryModal';
import { FulltextSearchPanel } from '@/components/Search/FulltextSearchPanel';
import {
  type CommandItem,
  GlobalSearch,
  type SearchResult,
} from '@/components/Search/GlobalSearch';
import {
  handleCloseConnectionModal,
  setFulltextSearchOpen,
  setLibraryModalOpen,
  setSearchOpen,
  setShowOnboarding,
  useModalStore,
} from '@/lib/modalStore';
import type { Namespace, SavedConnection, SearchFilter } from '@/lib/tauri';

const CONNECTION_MODAL_EXIT_DELAY_MS = 200;

interface AppOverlaysProps {
  onConnected: (sessionId: string, connection: SavedConnection) => void;
  onConnectionSaved: (connection: SavedConnection) => void;
  onSearchSelect: (result: SearchResult) => void | Promise<void>;
  onSelectLibraryQuery: (query: string) => void;
  onNavigateToTable: (namespace: Namespace, tableName: string, searchFilter?: SearchFilter) => void;
  paletteCommands: CommandItem[];
  paletteFeatures: CommandItem[];
  sessionId: string | null;
}

export const AppOverlays = memo(function AppOverlays({
  onConnected,
  onConnectionSaved,
  onSearchSelect,
  onSelectLibraryQuery,
  onNavigateToTable,
  paletteCommands,
  paletteFeatures,
  sessionId,
}: AppOverlaysProps) {
  const searchOpen = useModalStore(s => s.searchOpen);
  const fulltextSearchOpen = useModalStore(s => s.fulltextSearchOpen);
  const connectionModalOpen = useModalStore(s => s.connectionModalOpen);
  const libraryModalOpen = useModalStore(s => s.libraryModalOpen);
  const showOnboarding = useModalStore(s => s.showOnboarding);
  const editConnection = useModalStore(s => s.editConnection);
  const editPassword = useModalStore(s => s.editPassword);

  const [renderedEditConnection, setRenderedEditConnection] = useState<
    SavedConnection | undefined
  >();
  const [renderedEditPassword, setRenderedEditPassword] = useState<string | undefined>();

  useEffect(() => {
    if (connectionModalOpen) {
      setRenderedEditConnection(editConnection || undefined);
      setRenderedEditPassword(editPassword || undefined);
      return;
    }

    const timeoutId = window.setTimeout(() => {
      setRenderedEditConnection(undefined);
      setRenderedEditPassword(undefined);
    }, CONNECTION_MODAL_EXIT_DELAY_MS);

    return () => window.clearTimeout(timeoutId);
  }, [connectionModalOpen, editConnection, editPassword]);

  return (
    <>
      <ConnectionModal
        isOpen={connectionModalOpen}
        onClose={handleCloseConnectionModal}
        onConnected={onConnected}
        editConnection={renderedEditConnection}
        editPassword={renderedEditPassword}
        onSaved={onConnectionSaved}
      />
      <GlobalSearch
        isOpen={searchOpen}
        onClose={() => setSearchOpen(false)}
        onSelect={onSearchSelect}
        commands={paletteCommands}
        features={paletteFeatures}
      />
      <QueryLibraryModal
        isOpen={libraryModalOpen}
        onClose={() => setLibraryModalOpen(false)}
        onSelectQuery={onSelectLibraryQuery}
      />
      <FulltextSearchPanel
        isOpen={fulltextSearchOpen}
        onClose={() => setFulltextSearchOpen(false)}
        sessionId={sessionId}
        onNavigateToTable={onNavigateToTable}
      />
      {showOnboarding && <OnboardingModal onComplete={() => setShowOnboarding(false)} />}
    </>
  );
});
