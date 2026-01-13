import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Sidebar } from './components/Sidebar/Sidebar';
import { TabBar } from './components/Tabs/TabBar';
import { GlobalSearch } from './components/Search/GlobalSearch';
import { QueryPanel } from './components/Query/QueryPanel';
import { TableBrowser } from './components/Browser/TableBrowser';
import { ConnectionModal } from './components/Connection/ConnectionModal';
import { SettingsPage } from './components/Settings/SettingsPage';
import { Button } from './components/ui/button';
import { Search, Settings } from 'lucide-react';
import { Namespace, SavedConnection } from './lib/tauri';
import { Driver } from './lib/drivers';
import { Toaster } from 'sonner';
import { useTheme } from './hooks/useTheme';
import './index.css';

interface SelectedTable {
  namespace: Namespace;
  tableName: string;
}

function App() {
  const { t } = useTranslation();
  const { theme } = useTheme();
  const [searchOpen, setSearchOpen] = useState(false);
  const [connectionModalOpen, setConnectionModalOpen] = useState(false);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [driver, setDriver] = useState<Driver>('postgres');
  const [selectedTable, setSelectedTable] = useState<SelectedTable | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  
  // Edit connection state
  const [editConnection, setEditConnection] = useState<SavedConnection | null>(null);
  const [editPassword, setEditPassword] = useState<string>('');

  // Global keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setSearchOpen(true);
      }
      // Cmd+N: New connection
      if ((e.metaKey || e.ctrlKey) && e.key === 'n') {
        e.preventDefault();
        setConnectionModalOpen(true);
      }
      // Cmd+,: Settings
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault();
        setSettingsOpen(true);
      }
      // Escape: Close table browser or settings
      if (e.key === 'Escape') {
        if (selectedTable) setSelectedTable(null);
        if (settingsOpen) setSettingsOpen(false);
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [selectedTable, settingsOpen]);

  function handleConnected(newSessionId: string, newDriver: string) {
    setSessionId(newSessionId);
    setDriver(newDriver as Driver);
    setSelectedTable(null);
    setSettingsOpen(false);
  }

  function handleTableSelect(namespace: Namespace, tableName: string) {
    setSelectedTable({ namespace, tableName });
    setSettingsOpen(false);
  }

  function handleCloseTableBrowser() {
    setSelectedTable(null);
  }

  function handleEditConnection(connection: SavedConnection, password: string) {
    setEditConnection(connection);
    setEditPassword(password);
    setConnectionModalOpen(true);
  }

  function handleCloseConnectionModal() {
    setConnectionModalOpen(false);
    setEditConnection(null);
    setEditPassword('');
  }

  return (
    <>
      <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground font-sans">
        <Sidebar
          onNewConnection={() => setConnectionModalOpen(true)}
          onConnected={handleConnected}
          connectedSessionId={sessionId}
          onTableSelect={handleTableSelect}
          onEditConnection={handleEditConnection}
        />
        <main className="flex-1 flex flex-col min-w-0 bg-background relative">
          <header className="flex items-center justify-end absolute right-0 top-0 h-[40px] z-50 pr-2">
             <Button
               variant="ghost"
               size="icon"
               onClick={() => setSettingsOpen(!settingsOpen)}
               className="text-muted-foreground hover:text-foreground"
               title={t('settings.title')}
             >
               <Settings size={20} />
             </Button>
          </header>
          
          {!settingsOpen && <TabBar />}

          <div className="flex-1 overflow-auto p-4 pt-12">
            {settingsOpen ? (
              <SettingsPage />
            ) : sessionId ? (
              selectedTable ? (
                <TableBrowser
                  sessionId={sessionId}
                  namespace={selectedTable.namespace}
                  tableName={selectedTable.tableName}
                  onClose={handleCloseTableBrowser}
                />
              ) : (
                <QueryPanel sessionId={sessionId} dialect={driver} />
              )
            ) : (
              <div className="flex flex-col items-center justify-center h-full text-center space-y-4">
                <div className="p-4 rounded-full bg-accent/10 text-accent mb-4">
                  <img src="/logo.png" alt="QoreDB" width={48} height={48} />
                </div>
                <h2 className="text-2xl font-semibold tracking-tight">{t('app.welcome')}</h2>
                <p className="text-muted-foreground max-w-[400px]">
                  {t('app.description')}
                </p>
                <div className="flex flex-col gap-2 min-w-[200px]">
                  <Button 
                    onClick={() => setConnectionModalOpen(true)}
                    className="w-full"
                  >
                    + {t('app.newConnection')}
                  </Button>
                  <Button 
                    variant="outline" 
                    onClick={() => setSearchOpen(true)}
                    className="w-full text-muted-foreground"
                  >
                    <Search className="mr-2 h-4 w-4" />
                    {t('app.search')} <kbd className="ml-auto pointer-events-none inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground opacity-100"><span className="text-xs">⌘</span>K</kbd>
                  </Button>
                </div>
              </div>
            )}
          </div>
        </main>
      </div>

      <ConnectionModal
        isOpen={connectionModalOpen}
        onClose={handleCloseConnectionModal}
        onConnected={handleConnected}
        editConnection={editConnection || undefined}
        editPassword={editPassword || undefined}
        onSaved={handleCloseConnectionModal}
      />

      <GlobalSearch
        isOpen={searchOpen}
        onClose={() => setSearchOpen(false)}
      />

      <Toaster 
        theme={theme === 'dark' ? 'dark' : 'light'} 
        closeButton
        position="bottom-right"
        toastOptions={{
          className: 'bg-background border-border text-foreground',
          duration: 4000,
        }}
      />
    </>
  );
}

export default App;


