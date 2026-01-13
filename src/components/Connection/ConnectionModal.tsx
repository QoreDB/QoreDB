import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  testConnection, 
  connect, 
  saveConnection, 
  ConnectionConfig,
  SavedConnection
} from '../../lib/tauri';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { 
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter
} from '@/components/ui/dialog';
import { Check, X, Loader2, ChevronDown, ChevronRight } from 'lucide-react';
import { cn } from '@/lib/utils';
import { 
  Driver, 
  DRIVER_LABELS, 
  DRIVER_ICONS, 
  DEFAULT_PORTS,
  getDriverMetadata
} from '../../lib/drivers';
import { toast } from 'sonner';

interface ConnectionModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConnected: (sessionId: string, driver: string) => void;
  // Edit mode props
  editConnection?: SavedConnection;
  editPassword?: string;
  onSaved?: () => void;
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
  // SSH Tunnel
  useSshTunnel: boolean;
  sshHost: string;
  sshPort: number;
  sshUsername: string;
  sshKeyPath: string;
  sshPassphrase: string;
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
  // SSH Tunnel
  useSshTunnel: false,
  sshHost: '',
  sshPort: 22,
  sshUsername: '',
  sshKeyPath: '',
  sshPassphrase: '',
};

export function ConnectionModal({ 
  isOpen, 
  onClose, 
  onConnected,
  editConnection,
  editPassword,
  onSaved
}: ConnectionModalProps) {
  const { t } = useTranslation();
  const [formData, setFormData] = useState<FormData>(initialFormData);
  const [testing, setTesting] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [testResult, setTestResult] = useState<'success' | 'error' | null>(null);
  const [error, setError] = useState<string | null>(null);

  const isEditMode = !!editConnection;
  const driverMeta = getDriverMetadata(formData.driver);

  useEffect(() => {
    if (isOpen) {
      if (editConnection && editPassword) {
        // Populate form with existing connection data
        setFormData({
          name: editConnection.name,
          driver: editConnection.driver as Driver,
          host: editConnection.host,
          port: editConnection.port,
          username: editConnection.username,
          password: editPassword,
          database: editConnection.database || '',
          ssl: editConnection.ssl,
          // SSH Tunnel
          useSshTunnel: !!editConnection.ssh_tunnel,
          sshHost: editConnection.ssh_tunnel?.host || '',
          sshPort: editConnection.ssh_tunnel?.port || 22,
          sshUsername: editConnection.ssh_tunnel?.username || '',
          sshKeyPath: editConnection.ssh_tunnel?.key_path || '',
          sshPassphrase: '',
        });
      } else {
        setFormData(initialFormData);
      }
      setTestResult(null);
      setError(null);
    }
  }, [isOpen, editConnection, editPassword]);

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
        ssh_tunnel: formData.useSshTunnel ? {
          host: formData.sshHost,
          port: formData.sshPort,
          username: formData.sshUsername,
          auth: { Key: { private_key_path: formData.sshKeyPath, passphrase: formData.sshPassphrase || undefined } },
        } : undefined,
      };

      const result = await testConnection(config);
      
      if (result.success) {
        setTestResult('success');
        toast.success(t('connection.testSuccess'));
      } else {
        setTestResult('error');
        setError(result.error || t('connection.testFail'));
        toast.error(t('connection.testFail'), { description: result.error });
      }
    } catch (err) {
      setTestResult('error');
      const errorMsg = err instanceof Error ? err.message : t('common.error');
      setError(errorMsg);
      toast.error(t('connection.testFail'), { description: errorMsg });
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
        ssh_tunnel: formData.useSshTunnel ? {
          host: formData.sshHost,
          port: formData.sshPort,
          username: formData.sshUsername,
          auth: { Key: { private_key_path: formData.sshKeyPath, passphrase: formData.sshPassphrase || undefined } },
        } : undefined,
      };

      const connectionId = editConnection?.id || `conn_${Date.now()}`;
      
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
        ssh_tunnel: formData.useSshTunnel ? {
          host: formData.sshHost,
          port: formData.sshPort,
          username: formData.sshUsername,
          auth_type: 'key',
          key_path: formData.sshKeyPath,
          key_passphrase: formData.sshPassphrase || undefined,
        } : undefined,
      });

      if (isEditMode) {
        toast.success(t('connection.updateSuccess'));
        onSaved?.();
        onClose();
      } else {
        const connectResult = await connect(config);
        
        if (connectResult.success && connectResult.session_id) {
          toast.success(t('connection.connectedSuccess'));
          onConnected(connectResult.session_id, formData.driver);
          onClose();
        } else {
          setError(connectResult.error || t('connection.connectFail'));
          toast.error(t('connection.connectFail'), { description: connectResult.error });
        }
      }
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : t('common.error');
      setError(errorMsg);
      toast.error(t('common.error'), { description: errorMsg });
    } finally {
      setConnecting(false);
    }
  }

  async function handleSaveOnly() {
    setConnecting(true);
    setError(null);

    try {
      const connectionId = editConnection?.id || `conn_${Date.now()}`;
      
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
        ssh_tunnel: formData.useSshTunnel ? {
          host: formData.sshHost,
          port: formData.sshPort,
          username: formData.sshUsername,
          auth_type: 'key',
          key_path: formData.sshKeyPath,
          key_passphrase: formData.sshPassphrase || undefined,
        } : undefined,
      });

      toast.success(isEditMode ? t('connection.updateSuccess') : t('connection.saveSuccess'));
      onSaved?.();
      onClose();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : t('common.error');
      setError(errorMsg);
      toast.error(t('common.error'), { description: errorMsg });
    } finally {
      setConnecting(false);
    }
  }

  function handleOpenChange(open: boolean) {
    if (!open) {
      onClose();
    }
  }

  const isValid = formData.host && formData.username && formData.password && 
    (!formData.useSshTunnel || (formData.sshHost && formData.sshUsername && formData.sshKeyPath));

  return (
    <Dialog open={isOpen} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-lg max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {isEditMode ? t('connection.modalTitleEdit') : t('connection.modalTitleNew')}
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-6 py-4">
          <div className="grid grid-cols-3 gap-3">
            {(Object.keys(DRIVER_LABELS) as Driver[]).map(driver => (
              <button
                key={driver}
                className={cn(
                  "flex flex-col items-center gap-2 p-3 rounded-md border transition-all hover:bg-(--q-accent-soft)",
                  formData.driver === driver 
                    ? "border-accent bg-(--q-accent-soft) text-(--q-accent)" 
                    : "border-border bg-background"
                )}
                onClick={() => handleDriverChange(driver)}
                disabled={isEditMode}
              >
                <div className={cn(
                  "flex items-center justify-center w-10 h-10 rounded-lg p-1.5 transition-colors",
                  formData.driver === driver ? "bg-(--q-accent-soft)" : "bg-muted"
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
              <label className="text-sm font-medium">{t('connection.connectionName')}</label>
              <Input
                placeholder="My Database"
                value={formData.name}
                onChange={e => handleChange('name', e.target.value)}
              />
            </div>

            <div className="grid grid-cols-3 gap-4">
              <div className="col-span-2 space-y-2">
                <label className="text-sm font-medium">{t('connection.host')} <span className="text-error">*</span></label>
                <Input
                  placeholder="localhost"
                  value={formData.host}
                  onChange={e => handleChange('host', e.target.value)}
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium">{t('connection.port')}</label>
                <Input
                  type="number"
                  value={formData.port}
                  onChange={e => handleChange('port', parseInt(e.target.value) || 0)}
                />
              </div>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <label className="text-sm font-medium">{t('connection.username')} <span className="text-error">*</span></label>
                <Input
                  placeholder="user"
                  value={formData.username}
                  onChange={e => handleChange('username', e.target.value)}
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium">{t('connection.password')} <span className="text-error">*</span></label>
                <Input
                  type="password"
                  placeholder="••••••••"
                  value={formData.password}
                  onChange={e => handleChange('password', e.target.value)}
                />
              </div>
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium">{t(driverMeta.databaseFieldLabel)}</label>
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
              <label htmlFor="ssl" className="text-sm font-medium cursor-pointer">{t('connection.useSSL')}</label>
            </div>

            {/* SSH Tunnel Section */}
            <div className="border border-border rounded-md">
              <button
                type="button"
                className="flex items-center justify-between w-full px-3 py-2 text-sm font-medium hover:bg-muted/50 transition-colors"
                onClick={() => handleChange('useSshTunnel', !formData.useSshTunnel)}
              >
                <span className="flex items-center gap-2">
                  {formData.useSshTunnel ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
                  {t('connection.ssh.enableTunnel')}
                </span>
                <input
                  type="checkbox"
                  checked={formData.useSshTunnel}
                  onChange={e => { e.stopPropagation(); handleChange('useSshTunnel', e.target.checked); }}
                  className="h-4 w-4 rounded border-border text-accent focus:ring-accent"
                />
              </button>
              
              {formData.useSshTunnel && (
                <div className="px-3 pb-3 space-y-3 border-t border-border">
                  <div className="grid grid-cols-3 gap-3 pt-3">
                    <div className="col-span-2 space-y-1">
                      <label className="text-xs font-medium text-muted-foreground">{t('connection.ssh.host')}</label>
                      <Input
                        placeholder="bastion.example.com"
                        value={formData.sshHost}
                        onChange={e => handleChange('sshHost', e.target.value)}
                      />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs font-medium text-muted-foreground">{t('connection.ssh.port')}</label>
                      <Input
                        type="number"
                        value={formData.sshPort}
                        onChange={e => handleChange('sshPort', parseInt(e.target.value) || 22)}
                      />
                    </div>
                  </div>
                  
                  <div className="space-y-1">
                    <label className="text-xs font-medium text-muted-foreground">{t('connection.ssh.username')}</label>
                    <Input
                      placeholder="ssh_user"
                      value={formData.sshUsername}
                      onChange={e => handleChange('sshUsername', e.target.value)}
                    />
                  </div>
                  
                  <div className="space-y-1">
                    <label className="text-xs font-medium text-muted-foreground">{t('connection.ssh.keyPath')}</label>
                    <Input
                      placeholder={t('connection.ssh.keyPathPlaceholder')}
                      value={formData.sshKeyPath}
                      onChange={e => handleChange('sshKeyPath', e.target.value)}
                    />
                  </div>
                  
                  <div className="space-y-1">
                    <label className="text-xs font-medium text-muted-foreground">{t('connection.ssh.passphrase')}</label>
                    <Input
                      type="password"
                      placeholder="••••••••"
                      value={formData.sshPassphrase}
                      onChange={e => handleChange('sshPassphrase', e.target.value)}
                    />
                  </div>
                </div>
              )}
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
              {t('connection.testSuccess')}
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('connection.cancel')}
          </Button>
          <Button
            variant="secondary"
            onClick={handleTestConnection}
            disabled={!isValid || testing}
          >
            {testing && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            {t('connection.test')}
          </Button>
          {isEditMode ? (
            <Button
              onClick={handleSaveOnly}
              disabled={!isValid || connecting}
            >
              {connecting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {t('connection.saveChanges')}
            </Button>
          ) : (
            <Button
              onClick={handleSaveAndConnect}
              disabled={!isValid || connecting}
            >
              {connecting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {t('connection.saveConnect')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
