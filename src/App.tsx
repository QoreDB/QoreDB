import { useState, useEffect } from 'react';
import { MainLayout } from './components/Layout/MainLayout';
import { GlobalSearch } from './components/Search/GlobalSearch';
import { QueryPanel } from './components/Query/QueryPanel';
import './index.css';
import './App.css';

function App() {
  const [searchOpen, setSearchOpen] = useState(false);
  // TODO: Wire up connection selection from sidebar
  const activeSessionId: string | null = null;

  // Global keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      // Cmd+K or Ctrl+K: Open search
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setSearchOpen(true);
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  return (
    <>
      <MainLayout>
        {activeSessionId ? (
          <QueryPanel sessionId={activeSessionId} />
        ) : (
          <div className="welcome-screen">
            <h2>Welcome to QoreDB</h2>
            <p className="text-muted">
              Add a connection in the sidebar to get started, or press{' '}
              <kbd>Cmd+K</kbd> to search.
            </p>
            
            {/* Demo: show query panel without connection */}
            <div style={{ marginTop: '2rem', width: '100%', maxWidth: '800px' }}>
              <p className="text-muted" style={{ marginBottom: '1rem' }}>
                Preview (no connection):
              </p>
              <QueryPanel sessionId={null} />
            </div>
          </div>
        )}
      </MainLayout>
      
      <GlobalSearch
        isOpen={searchOpen}
        onClose={() => setSearchOpen(false)}
      />
    </>
  );
}

export default App;
