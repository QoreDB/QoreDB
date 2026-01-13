import { useState, useRef, useEffect } from 'react';
import { SavedConnection, deleteSavedConnection, testConnection, getConnectionCredentials, ConnectionConfig } from '../../lib/tauri';
import { Button } from '@/components/ui/button';
import { 
  MoreVertical, 
  Pencil, 
  Trash2, 
  Zap, 
  Copy,
  Loader2
} from 'lucide-react';
import { toast } from 'sonner';

interface ConnectionMenuProps {
  connection: SavedConnection;
  onEdit: (connection: SavedConnection, password: string) => void;
  onDeleted: () => void;
}

export function ConnectionMenu({ connection, onEdit, onDeleted }: ConnectionMenuProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [testing, setTesting] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  // Close menu when clicking outside
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    }
    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      return () => document.removeEventListener('mousedown', handleClickOutside);
    }
  }, [isOpen]);

  async function handleTest() {
    setTesting(true);
    try {
      // Get password from vault
      const credsResult = await getConnectionCredentials('default', connection.id);
      if (!credsResult.success || !credsResult.password) {
        toast.error('Failed to retrieve credentials');
        return;
      }

      const config: ConnectionConfig = {
        driver: connection.driver,
        host: connection.host,
        port: connection.port,
        username: connection.username,
        password: credsResult.password,
        database: connection.database,
        ssl: connection.ssl,
      };

      const result = await testConnection(config);
      
      if (result.success) {
        toast.success(`Connection to ${connection.name} successful!`, {
          description: `${connection.host}:${connection.port}`,
        });
      } else {
        toast.error(`Connection failed`, {
          description: result.error || 'Unknown error',
        });
      }
    } catch (err) {
      toast.error('Test failed', {
        description: err instanceof Error ? err.message : 'Unknown error',
      });
    } finally {
      setTesting(false);
      setIsOpen(false);
    }
  }

  async function handleEdit() {
    try {
      // Get password from vault for editing
      const credsResult = await getConnectionCredentials('default', connection.id);
      if (!credsResult.success || !credsResult.password) {
        toast.error('Failed to retrieve credentials for editing');
        return;
      }
      onEdit(connection, credsResult.password);
      setIsOpen(false);
    } catch (err) {
      toast.error('Failed to load connection details');
    }
  }

  async function handleDelete() {
    if (!confirm(`Delete connection "${connection.name}"? This cannot be undone.`)) {
      return;
    }

    setDeleting(true);
    try {
      const result = await deleteSavedConnection('default', connection.id);
      if (result.success) {
        toast.success(`Connection "${connection.name}" deleted`);
        onDeleted();
      } else {
        toast.error('Failed to delete connection', {
          description: result.error,
        });
      }
    } catch (err) {
      toast.error('Delete failed', {
        description: err instanceof Error ? err.message : 'Unknown error',
      });
    } finally {
      setDeleting(false);
      setIsOpen(false);
    }
  }

  function handleDuplicate() {
    // For now just show a message - could be implemented later
    toast.info('Duplicate feature coming soon');
    setIsOpen(false);
  }

  return (
    <div className="relative" ref={menuRef}>
      <Button
        variant="ghost"
        size="icon"
        className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"
        onClick={(e) => {
          e.stopPropagation();
          setIsOpen(!isOpen);
        }}
      >
        <MoreVertical size={14} />
      </Button>

      {isOpen && (
        <div 
          className="absolute right-0 top-full mt-1 z-50 min-w-[160px] bg-background border border-border rounded-md shadow-lg py-1 animate-in fade-in-0 zoom-in-95"
          onClick={(e) => e.stopPropagation()}
        >
          <button
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-muted transition-colors text-left"
            onClick={handleTest}
            disabled={testing}
          >
            {testing ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <Zap size={14} />
            )}
            Test Connection
          </button>

          <button
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-muted transition-colors text-left"
            onClick={handleEdit}
          >
            <Pencil size={14} />
            Edit
          </button>

          <button
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-muted transition-colors text-left"
            onClick={handleDuplicate}
          >
            <Copy size={14} />
            Duplicate
          </button>

          <div className="h-px bg-border my-1" />

          <button
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-error/10 text-error transition-colors text-left"
            onClick={handleDelete}
            disabled={deleting}
          >
            {deleting ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <Trash2 size={14} />
            )}
            Delete
          </button>
        </div>
      )}
    </div>
  );
}
