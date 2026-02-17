// SPDX-License-Identifier: Apache-2.0

import { createContext, useContext, useState, useCallback, useEffect, type ReactNode } from 'react';
import type { SavedConnection } from '@/lib/tauri';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';

export interface ModalContextValue {
  searchOpen: boolean;
  fulltextSearchOpen: boolean;
  connectionModalOpen: boolean;
  libraryModalOpen: boolean;
  settingsOpen: boolean;
  sidebarVisible: boolean;
  showOnboarding: boolean;
  editConnection: SavedConnection | null;
  editPassword: string;
  setSearchOpen: (open: boolean) => void;
  setFulltextSearchOpen: (open: boolean) => void;
  setConnectionModalOpen: (open: boolean) => void;
  setLibraryModalOpen: (open: boolean) => void;
  setSettingsOpen: (open: boolean) => void;
  setSidebarVisible: (visible: boolean) => void;
  setShowOnboarding: (show: boolean) => void;
  handleEditConnection: (connection: SavedConnection, password: string) => void;
  handleCloseConnectionModal: () => void;
  toggleSidebar: () => void;
}

const ModalContext = createContext<ModalContextValue | null>(null);

export function ModalProvider({ children }: { children: ReactNode }) {
  const [searchOpen, setSearchOpen] = useState(false);
  const [fulltextSearchOpen, setFulltextSearchOpen] = useState(false);
  const [connectionModalOpen, setConnectionModalOpen] = useState(false);
  const [libraryModalOpen, setLibraryModalOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [editConnection, setEditConnection] = useState<SavedConnection | null>(null);
  const [editPassword, setEditPassword] = useState('');

  useEffect(() => {
    if (!AnalyticsService.isOnboardingCompleted()) {
      setShowOnboarding(true);
    }
  }, []);

  const handleEditConnection = useCallback((connection: SavedConnection, password: string) => {
    setEditConnection(connection);
    setEditPassword(password);
    setConnectionModalOpen(true);
  }, []);

  const handleCloseConnectionModal = useCallback(() => {
    setConnectionModalOpen(false);
    setEditConnection(null);
    setEditPassword('');
  }, []);

  const toggleSidebar = useCallback(() => {
    setSidebarVisible(prev => !prev);
  }, []);

  return (
    <ModalContext.Provider
      value={{
        searchOpen,
        fulltextSearchOpen,
        connectionModalOpen,
        libraryModalOpen,
        settingsOpen,
        sidebarVisible,
        showOnboarding,
        editConnection,
        editPassword,
        setSearchOpen,
        setFulltextSearchOpen,
        setConnectionModalOpen,
        setLibraryModalOpen,
        setSettingsOpen,
        setSidebarVisible,
        setShowOnboarding,
        handleEditConnection,
        handleCloseConnectionModal,
        toggleSidebar,
      }}
    >
      {children}
    </ModalContext.Provider>
  );
}

export function useModalContext(): ModalContextValue {
  const ctx = useContext(ModalContext);
  if (!ctx) throw new Error('useModalContext must be used within ModalProvider');
  return ctx;
}
