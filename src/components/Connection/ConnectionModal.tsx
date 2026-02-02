import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { DRIVER_ICONS, DRIVER_LABELS } from '@/lib/drivers';
import { cn } from '@/lib/utils';

import {
  connectSavedConnection,
  saveConnection,
  testConnection,
  type SavedConnection,
} from '@/lib/tauri';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Switch } from '@/components/ui/switch';
import { Check, Link2, Loader2, X } from 'lucide-react';
import { toast } from 'sonner';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';

import { DriverPicker } from './connection-modal/DriverPicker';
import { UrlInput } from './connection-modal/UrlSection';
import { BasicSection } from './connection-modal/BasicSection';
import { AdvancedSection } from './connection-modal/AdvancedSection';
import {
  buildConnectionConfig,
  buildSaveConnectionInput,
  buildSavedConnection,
} from './connection-modal/mappers';
import { useConnectionForm } from './connection-modal/useConnectionForm';

interface ConnectionModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConnected: (sessionId: string, connection: SavedConnection) => void;
  editConnection?: SavedConnection;
  editPassword?: string;
  onSaved?: (connection: SavedConnection) => void;
}

export function ConnectionModal({
  isOpen,
  onClose,
  onConnected,
  editConnection,
  editPassword,
  onSaved,
}: ConnectionModalProps) {
  const { t } = useTranslation();
  const {
    formData,
    handleChange: setField,
    handleDriverChange,
    applyParsedConfig,
    isValid,
  } = useConnectionForm({ isOpen, editConnection, editPassword });
  const [testing, setTesting] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [testResult, setTestResult] = useState<'success' | 'error' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [urlParsed, setUrlParsed] = useState(false);

  // Step 1: "driver" | Step 2: "form"
  const [step, setStep] = useState<'driver' | 'form'>('driver');

  const isEditMode = !!editConnection;

  // Initialize step based on mode
  if (step === 'driver' && isEditMode) {
    setStep('form');
  }

  const hideConnectionFields = formData.useUrl && urlParsed;

  function handleDriverSelect(nextDriver: Parameters<typeof handleDriverChange>[0]) {
    handleDriverChange(nextDriver);
    setTestResult(null);
    setError(null);
    setStep('form');
  }

  function handleBackToDriver() {
    setStep('driver');
    setTestResult(null);
    setError(null);
  }

  function handleChange(
    field: Parameters<typeof setField>[0],
    value: Parameters<typeof setField>[1]
  ) {
    setField(field, value);
    setTestResult(null);
    setError(null);
    // Reset URL
    if (field === 'useUrl' && !value) {
      setUrlParsed(false);
    }
  }

  const handleParseStatusChange = useCallback((isParsed: boolean) => {
    setUrlParsed(isParsed);
  }, []);

  async function handleTestConnection() {
    setTesting(true);
    setTestResult(null);
    setError(null);

    try {
      const config = buildConnectionConfig(formData);

      const result = await testConnection(config);

      if (result.success) {
        setTestResult('success');
        toast.success(t('connection.testSuccess'));
        AnalyticsService.capture('connection_tested_success', {
          source: 'modal',
          driver: formData.driver,
        });
      } else {
        AnalyticsService.capture('connection_tested_failed', {
          source: 'modal',
          driver: formData.driver,
        });
        setTestResult('error');
        setError(result.error || t('connection.testFail'));
        toast.error(t('connection.testFail'), { description: result.error });
      }
    } catch (err) {
      AnalyticsService.capture('connection_tested_failed', {
        source: 'modal',
        driver: formData.driver,
      });
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
      const connectionId = editConnection?.id || `conn_${Date.now()}`;
      const savedConnection = buildSavedConnection(formData, connectionId);
      await saveConnection(buildSaveConnectionInput(formData, connectionId));
      if (!isEditMode) {
        AnalyticsService.capture('connection_created', {
          source: 'modal',
          driver: formData.driver,
        });
      }

      if (isEditMode) {
        toast.success(t('connection.updateSuccess'));
        onSaved?.(savedConnection);
        onClose();
      } else {
        const connectResult = await connectSavedConnection('default', connectionId);

        if (connectResult.success && connectResult.session_id) {
          toast.success(t('connection.connectedSuccess'));
          AnalyticsService.capture('connected_success', {
            source: 'modal',
            driver: formData.driver,
          });
          onConnected(connectResult.session_id, savedConnection);
          onClose();
        } else {
          AnalyticsService.capture('connected_failed', {
            source: 'modal',
            driver: formData.driver,
          });
          setError(connectResult.error || t('connection.connectFail'));
          toast.error(t('connection.connectFail'), {
            description: connectResult.error,
          });
        }
      }
    } catch (err) {
      AnalyticsService.capture('connected_failed', {
        source: 'modal',
        driver: formData.driver,
      });
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
      const savedConnection = buildSavedConnection(formData, connectionId);
      await saveConnection(buildSaveConnectionInput(formData, connectionId));
      if (!isEditMode) {
        AnalyticsService.capture('connection_created', {
          source: 'modal',
          driver: formData.driver,
        });
      }

      toast.success(isEditMode ? t('connection.updateSuccess') : t('connection.saveSuccess'));
      onSaved?.(savedConnection);
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
      // TODO: handle this better
      if (!isEditMode) setTimeout(() => setStep('driver'), 200);
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleOpenChange}>
      <DialogContent
        className={cn('max-w-xl duration-200', step === 'driver' ? 'max-w-3xl' : 'max-w-xl')}
      >
        <DialogHeader>
          <DialogTitle>
            {isEditMode
              ? t('connection.modalTitleEdit')
              : step === 'driver'
                ? t('connection.selectDriver')
                : t('connection.configureConnection')}
          </DialogTitle>
        </DialogHeader>

        {step === 'driver' ? (
          <div className="py-6">
            <DriverPicker
              driver={formData.driver}
              isEditMode={isEditMode}
              onChange={handleDriverSelect}
            />
            <div className="mt-6 flex justify-end">
              <Button variant="outline" onClick={onClose}>
                {t('connection.cancel')}
              </Button>
            </div>
          </div>
        ) : (
          <>
            <ScrollArea className="max-h-[75vh]">
              <div className="grid gap-4 py-4">
                {/* Driver Header with URL toggle */}
                <div className="flex items-center justify-between p-3 rounded-md bg-muted/30 border border-border">
                  <div className="flex items-center gap-3">
                    <div className="w-8 h-8 rounded p-1 bg-background border border-border flex items-center justify-center">
                      <img
                        src={`/databases/${DRIVER_ICONS[formData.driver]}`}
                        alt={DRIVER_LABELS[formData.driver]}
                        className="w-full h-full object-contain"
                      />
                    </div>
                    <div className="flex flex-col">
                      <span className="text-sm font-semibold">
                        {DRIVER_LABELS[formData.driver]}
                      </span>
                      <span className="text-xs text-muted-foreground">
                        {t('connection.driverSelected')}
                      </span>
                    </div>
                  </div>

                  <div className="flex items-center gap-3">
                    {/* URL Mode Toggle - only for new connections */}
                    {!isEditMode && (
                      <label className="flex items-center gap-2 cursor-pointer">
                        <Link2
                          size={14}
                          className={cn(
                            'transition-colors',
                            formData.useUrl ? 'text-primary' : 'text-muted-foreground'
                          )}
                        />
                        <span
                          className={cn(
                            'text-xs transition-colors',
                            formData.useUrl ? 'text-primary font-medium' : 'text-muted-foreground'
                          )}
                        >
                          URL
                        </span>
                        <Switch
                          checked={formData.useUrl}
                          onCheckedChange={checked => handleChange('useUrl', checked)}
                          className="scale-90"
                        />
                      </label>
                    )}

                    {!isEditMode && <div className="w-px h-6 bg-border" />}

                    {!isEditMode && (
                      <Button variant="ghost" size="sm" onClick={handleBackToDriver}>
                        {t('connection.changeDriver')}
                      </Button>
                    )}
                  </div>
                </div>

                {/* URL Input - shown when URL mode is active */}
                {formData.useUrl && !isEditMode && (
                  <div className="rounded-md border border-border bg-background p-4">
                    <UrlInput
                      formData={formData}
                      onChange={handleChange}
                      onParsedConfig={applyParsedConfig}
                      onParseStatusChange={handleParseStatusChange}
                    />
                  </div>
                )}

                <BasicSection
                  formData={formData}
                  onChange={handleChange}
                  hideConnectionFields={hideConnectionFields}
                />
                <AdvancedSection
                  formData={formData}
                  onChange={handleChange}
                  hideUrlDerivedFields={hideConnectionFields}
                />

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
            </ScrollArea>

            <DialogFooter>
              <Button variant="outline" onClick={onClose}>
                {t('connection.cancel')}
              </Button>
              <Button
                variant="secondary"
                className="transition-all"
                onClick={handleTestConnection}
                disabled={!isValid || testing}
              >
                {testing && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                {t('connection.test')}
              </Button>
              {isEditMode ? (
                <div title={!isValid ? t('connection.validationError') : undefined}>
                  <Button onClick={handleSaveOnly} disabled={!isValid || connecting}>
                    {connecting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                    {t('connection.saveChanges')}
                  </Button>
                </div>
              ) : (
                <div title={!isValid ? t('connection.validationError') : undefined}>
                  <Button onClick={handleSaveAndConnect} disabled={!isValid || connecting}>
                    {connecting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                    {t('connection.saveConnect')}
                  </Button>
                </div>
              )}
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
