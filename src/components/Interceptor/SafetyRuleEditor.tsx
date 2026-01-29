/**
 * Safety Rule Editor
 *
 * Modal component for creating and editing custom safety rules
 */

import { useState, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Checkbox } from '../ui/checkbox';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../ui/select';
import type { SafetyRule, QueryOperationType, Environment } from '../../lib/tauri/interceptor';

interface SafetyRuleEditorProps {
  rule: SafetyRule | null;
  onSave: (rule: SafetyRule) => void;
  onCancel: () => void;
}

const OPERATION_TYPES: QueryOperationType[] = [
  'select',
  'insert',
  'update',
  'delete',
  'create',
  'alter',
  'drop',
  'truncate',
  'grant',
  'revoke',
  'execute',
  'other',
];

const ENVIRONMENTS: Environment[] = ['development', 'staging', 'production'];

const ACTIONS: Array<{ value: SafetyRule['action']; label: string }> = [
  { value: 'block', label: 'interceptor.safety.actions.block' },
  { value: 'warn', label: 'interceptor.safety.actions.warn' },
  { value: 'require_confirmation', label: 'interceptor.safety.actions.require_confirmation' },
];

export function SafetyRuleEditor({ rule, onSave, onCancel }: SafetyRuleEditorProps) {
  const { t } = useTranslation();
  const isEditing = !!rule;

  const [name, setName] = useState(rule?.name || '');
  const [description, setDescription] = useState(rule?.description || '');
  const [enabled, setEnabled] = useState(rule?.enabled ?? true);
  const [environments, setEnvironments] = useState<Environment[]>(
    rule?.environments || ['production']
  );
  const [operations, setOperations] = useState<QueryOperationType[]>(
    rule?.operations || []
  );
  const [action, setAction] = useState<SafetyRule['action']>(rule?.action || 'block');
  const [pattern, setPattern] = useState(rule?.pattern || '');
  const [patternError, setPatternError] = useState<string | null>(null);

  // Validate regex pattern
  const validatePattern = useCallback((value: string) => {
    if (!value.trim()) {
      setPatternError(null);
      return true;
    }
    try {
      new RegExp(value, 'i');
      setPatternError(null);
      return true;
    } catch (err) {
      setPatternError(err instanceof Error ? err.message : 'Invalid regex');
      return false;
    }
  }, []);

  const handlePatternChange = useCallback(
    (value: string) => {
      setPattern(value);
      validatePattern(value);
    },
    [validatePattern]
  );

  const handleEnvironmentToggle = useCallback((env: Environment, checked: boolean) => {
    setEnvironments(prev =>
      checked ? [...prev, env] : prev.filter(e => e !== env)
    );
  }, []);

  const handleOperationToggle = useCallback((op: QueryOperationType, checked: boolean) => {
    setOperations(prev =>
      checked ? [...prev, op] : prev.filter(o => o !== op)
    );
  }, []);

  const isValid = useMemo(() => {
    return (
      name.trim().length > 0 &&
      environments.length > 0 &&
      !patternError
    );
  }, [name, environments, patternError]);

  const handleSave = useCallback(() => {
    if (!isValid) return;

    const newRule: SafetyRule = {
      id: rule?.id || `custom-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      name: name.trim(),
      description: description.trim(),
      enabled,
      environments,
      operations,
      action,
      pattern: pattern.trim() || undefined,
      builtin: false,
    };

    onSave(newRule);
  }, [rule, name, description, enabled, environments, operations, action, pattern, isValid, onSave]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onCancel}
      />

      {/* Modal */}
      <div className="relative bg-background rounded-lg shadow-xl border border-border w-full max-w-lg mx-4 max-h-[90vh] overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-border">
          <h2 className="text-lg font-semibold">
            {isEditing ? t('interceptor.safety.editRule') : t('interceptor.safety.addRule')}
          </h2>
          <button
            type="button"
            onClick={onCancel}
            className="p-1 rounded hover:bg-muted transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-4 space-y-4">
          {/* Name */}
          <div className="space-y-2">
            <Label htmlFor="rule-name">{t('interceptor.safety.ruleFields.name')}</Label>
            <Input
              id="rule-name"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="Block dangerous queries"
            />
          </div>

          {/* Description */}
          <div className="space-y-2">
            <Label htmlFor="rule-description">{t('interceptor.safety.ruleFields.description')}</Label>
            <Input
              id="rule-description"
              value={description}
              onChange={e => setDescription(e.target.value)}
              placeholder="Prevents accidental data loss"
            />
          </div>

          {/* Enabled */}
          <div className="flex items-center gap-2">
            <Checkbox
              id="rule-enabled"
              checked={enabled}
              onCheckedChange={checked => setEnabled(!!checked)}
            />
            <Label htmlFor="rule-enabled">{t('interceptor.safety.ruleFields.enabled')}</Label>
          </div>

          {/* Environments */}
          <div className="space-y-2">
            <Label>{t('interceptor.safety.ruleFields.environments')}</Label>
            <div className="flex flex-wrap gap-3">
              {ENVIRONMENTS.map(env => (
                <label key={env} className="flex items-center gap-2 text-sm">
                  <Checkbox
                    checked={environments.includes(env)}
                    onCheckedChange={checked => handleEnvironmentToggle(env, !!checked)}
                  />
                  {t(`environment.${env}`)}
                </label>
              ))}
            </div>
          </div>

          {/* Operations */}
          <div className="space-y-2">
            <Label>{t('interceptor.safety.ruleFields.operations')}</Label>
            <p className="text-xs text-muted-foreground">
              Leave empty to match all operations
            </p>
            <div className="grid grid-cols-3 gap-2">
              {OPERATION_TYPES.map(op => (
                <label key={op} className="flex items-center gap-2 text-sm">
                  <Checkbox
                    checked={operations.includes(op)}
                    onCheckedChange={checked => handleOperationToggle(op, !!checked)}
                  />
                  {t(`interceptor.operations.${op}`)}
                </label>
              ))}
            </div>
          </div>

          {/* Action */}
          <div className="space-y-2">
            <Label>{t('interceptor.safety.ruleFields.action')}</Label>
            <Select value={action} onValueChange={v => setAction(v as SafetyRule['action'])}>
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ACTIONS.map(({ value, label }) => (
                  <SelectItem key={value} value={value}>
                    {t(label)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* Pattern */}
          <div className="space-y-2">
            <Label htmlFor="rule-pattern">{t('interceptor.safety.ruleFields.pattern')}</Label>
            <p className="text-xs text-muted-foreground">
              Optional regex to match query text
            </p>
            <Input
              id="rule-pattern"
              value={pattern}
              onChange={e => handlePatternChange(e.target.value)}
              placeholder="DROP\s+TABLE"
              className={patternError ? 'border-destructive' : ''}
            />
            {patternError && (
              <p className="text-xs text-destructive">{patternError}</p>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2 p-4 border-t border-border">
          <Button variant="outline" onClick={onCancel}>
            {t('common.cancel')}
          </Button>
          <Button onClick={handleSave} disabled={!isValid}>
            {t('common.save')}
          </Button>
        </div>
      </div>
    </div>
  );
}
