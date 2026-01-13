import { useState, useEffect } from 'react';
import { DBTree } from '../Tree/DBTree';
import { listSavedConnections, SavedConnection } from '../../lib/tauri';
import { useTheme } from '../../hooks/useTheme';
import './Sidebar.css';
import { ConnectionItem } from './ConnectionItem'

const DEFAULT_PROJECT = 'default';

export function Sidebar() {
  const [connections, setConnections] = useState<SavedConnection[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const { isDark, toggleTheme } = useTheme();

  useEffect(() => {
    loadConnections();
  }, []);

  async function loadConnections() {
    try {
      const saved = await listSavedConnections(DEFAULT_PROJECT);
      setConnections(saved);
    } catch (err) {
      console.error('Failed to load connections:', err);
    }
  }

  function handleSelect(id: string) {
    setSelectedId(id);
    setExpandedId(expandedId === id ? null : id);
  }

  return (
    <aside className="sidebar">
      <header className="sidebar-header">
        <h1 className="sidebar-title">QoreDB</h1>
        <button
          className="sidebar-theme-toggle"
          onClick={toggleTheme}
          title={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
        >
          {isDark ? '☀️' : '🌙'}
        </button>
      </header>

      <section className="sidebar-section">
        <h2 className="sidebar-section-title">Connections</h2>
        <div className="sidebar-connections">
          {connections.length === 0 ? (
            <p className="sidebar-empty">No saved connections</p>
          ) : (
            connections.map(conn => (
              <div key={conn.id}>
                <ConnectionItem
                  connection={conn}
                  isSelected={selectedId === conn.id}
                  isExpanded={expandedId === conn.id}
                  onSelect={() => handleSelect(conn.id)}
                />
                {expandedId === conn.id && (
                  <DBTree connectionId={conn.id} />
                )}
              </div>
            ))
          )}
        </div>
      </section>

      <footer className="sidebar-footer">
        <button className="sidebar-add-btn">+ New Connection</button>
      </footer>
    </aside>
  );
}
