import { useState, useEffect } from 'react';
import { 
  testConnection, 
  connect, 
  saveConnection, 
  ConnectionConfig 
} from '../../lib/tauri';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Check, X, Loader2 } from 'lucide-react';
import { cn } from '@/lib/utils';
import { 
  Driver, 
  DRIVER_LABELS, 
  DRIVER_ICONS, 
  DEFAULT_PORTS 
} from '../../lib/drivers';

interface ConnectionModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConnected: (sessionId: string, driver: string) => void;
}

interface FormData {
  name: string;
  driver: Driver;
  host: string;
  port: number;
  username: string;
  password: string;
  database: string;
  ssl: boolean;
}

const initialFormData: FormData = {
  name: '',
  driver: 'postgres',
  host: 'localhost',
  port: 5432,
  username: '',
  password: '',
  database: '',
  ssl: false,
};

export function ConnectionModal({ isOpen, onClose, onConnected }: ConnectionModalProps) {
  const [formData, setFormData] = useState<FormData>(initialFormData);
  const [testing, setTesting] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [testResult, setTestResult] = useState<'success' | 'error' | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      setFormData(initialFormData);
      setTestResult(null);
      setError(null);
    }
  }, [isOpen]);

  function handleDriverChange(driver: Driver) {
    setFormData(prev => ({
      ...prev,
      driver,
      port: DEFAULT_PORTS[driver],
    }));
    setTestResult(null);
    setError(null);
  }

  function handleChange(field: keyof FormData, value: string | number | boolean) {
    setFormData(prev => ({ ...prev, [field]: value }));
    setTestResult(null);
    setError(null);
  }

  async function handleTestConnection() {
    setTesting(true);
    setTestResult(null);
    setError(null);

    try {
      const config: ConnectionConfig = {
        driver: formData.driver,
        host: formData.host,
        port: formData.port,
        username: formData.username,
        password: formData.password,
        database: formData.database || undefined,
        ssl: formData.ssl,
      };

      const result = await testConnection(config);
      
      if (result.success) {
        setTestResult('success');
      } else {
        setTestResult('error');
        setError(result.error || 'Connection failed');
      }
    } catch (err) {
      setTestResult('error');
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setTesting(false);
    }
  }

  async function handleSaveAndConnect() {
    setConnecting(true);
    setError(null);

    try {
      const config: ConnectionConfig = {
        driver: formData.driver,
        host: formData.host,
        port: formData.port,
        username: formData.username,
        password: formData.password,
        database: formData.database || undefined,
        ssl: formData.ssl,
      };

      const connectionId = `conn_${Date.now()}`;
      await saveConnection({
        id: connectionId,
        name: formData.name || `${formData.host}:${formData.port}`,
        driver: formData.driver,
        host: formData.host,
        port: formData.port,
        username: formData.username,
        password: formData.password,
        database: formData.database || undefined,
        ssl: formData.ssl,
        project_id: 'default',
      });

      const connectResult = await connect(config);
      
      if (connectResult.success && connectResult.session_id) {
        onConnected(connectResult.session_id, formData.driver);
        onClose();
      } else {
        setError(connectResult.error || 'Failed to connect');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setConnecting(false);
    }
  }

  if (!isOpen) return null;

  const isValid = formData.host && formData.username && formData.password;

  return (
    <div className="fixed inset-0 z-50 bg-background/80 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="w-full max-w-lg bg-background border border-border rounded-lg shadow-lg flex flex-col max-h-[90vh]">
        <header className="flex items-center justify-between p-4 border-b border-border">
          <h2 className="text-lg font-semibold">New Connection</h2>
          <Button variant="ghost" size="icon" onClick={onClose} className="h-8 w-8">
            <X size={18} />
          </Button>
        </header>

        <div className="p-6 space-y-6 overflow-y-auto">
          {/* Driver Selection */}
          <div className="grid grid-cols-3 gap-3">
            {(Object.keys(DRIVER_LABELS) as Driver[]).map(driver => (
              <button
                key={driver}
                className={cn(
                  "flex flex-col items-center gap-2 p-3 rounded-md border transition-all hover:bg-muted",
                  formData.driver === driver 
                    ? "border-accent bg-accent/5 text-accent" 
                    : "border-border bg-background"
                )}
                onClick={() => handleDriverChange(driver)}
              >
                <div className={cn(
                  "flex items-center justify-center w-10 h-10 rounded-lg p-1.5 transition-colors",
                  formData.driver === driver ? "bg-accent/20" : "bg-muted"
                )}>
                  <img 
                    src={`/databases/${DRIVER_ICONS[driver]}`} 
                    alt={DRIVER_LABELS[driver]}
                    className="w-full h-full object-contain"
                  />
                </div>
                <span className="text-xs font-medium">{DRIVER_LABELS[driver]}</span>
              </button>
            ))}
          </div>

          <div className="space-y-4">
            <div className="space-y-2">
              <label className="text-sm font-medium">Connection Name</label>
              <Input
                placeholder="My Database"
                value={formData.name}
                onChange={e => handleChange('name', e.target.value)}
              />
            </div>

            <div className="grid grid-cols-3 gap-4">
              <div className="col-span-2 space-y-2">
                <label className="text-sm font-medium">Host <span className="text-error">*</span></label>
                <Input
                  placeholder="localhost"
                  value={formData.host}
                  onChange={e => handleChange('host', e.target.value)}
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium">Port</label>
                <Input
                  type="number"
                  value={formData.port}
                  onChange={e => handleChange('port', parseInt(e.target.value) || 0)}
                />
              </div>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <label className="text-sm font-medium">Username <span className="text-error">*</span></label>
                <Input
                  placeholder="user"
                  value={formData.username}
                  onChange={e => handleChange('username', e.target.value)}
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium">Password <span className="text-error">*</span></label>
                <Input
                  type="password"
                  placeholder="••••••••"
                  value={formData.password}
                  onChange={e => handleChange('password', e.target.value)}
                />
              </div>
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium">Database</label>
              <Input
                placeholder={formData.driver === 'postgres' ? 'postgres' : ''}
                value={formData.database}
                onChange={e => handleChange('database', e.target.value)}
              />
            </div>

            <div className="flex items-center space-x-2">
              <input
                type="checkbox"
                id="ssl"
                className="h-4 w-4 rounded border-border text-accent focus:ring-accent"
                checked={formData.ssl}
                onChange={e => handleChange('ssl', e.target.checked)}
              />
              <label htmlFor="ssl" className="text-sm font-medium cursor-pointer">Use SSL/TLS</label>
            </div>
          </div>

          {error && (
            <div className="p-3 rounded-md bg-error/10 border border-error/20 text-error text-sm flex items-center gap-2">
              <X size={14} />
              {error}
            </div>
          )}
          {testResult === 'success' && (
            <div className="p-3 rounded-md bg-success/10 border border-success/20 text-success text-sm flex items-center gap-2">
              <Check size={14} />
              Connection successful!
            </div>
          )}
        </div>

        <footer className="p-4 border-t border-border bg-muted/20 flex justify-end gap-2 rounded-b-lg">
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="secondary"
            onClick={handleTestConnection}
            disabled={!isValid || testing}
          >
            {testing && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Test Connection
          </Button>
          <Button
            onClick={handleSaveAndConnect}
            disabled={!isValid || connecting}
          >
            {connecting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Save & Connect
          </Button>
        </footer>
      </div>
    </div>
  );
}
