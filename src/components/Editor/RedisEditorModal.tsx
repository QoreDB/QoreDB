// SPDX-License-Identifier: Apache-2.0

import { Loader2 } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { DangerConfirmDialog } from '@/components/Guard/DangerConfirmDialog';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { executeQuery } from '@/lib/tauri';
import {
  type ListSide,
  type RedisKeyType,
  buildAddSetMember,
  buildDeleteHashField,
  buildDeleteKeys,
  buildPushListItem,
  buildRemoveSetMember,
  buildRemoveZSetMember,
  buildSetHashField,
  buildSetString,
  buildSetZSetMember,
} from '@/lib/redisCommands';

export type RedisEditorMode =
  | { kind: 'create-key' }
  | { kind: 'delete-key'; keys: string[] }
  | { kind: 'edit-value'; keyType: RedisKeyType; key: string; initialValue?: string }
  | { kind: 'add-hash-field'; key: string }
  | { kind: 'edit-hash-field'; key: string; field: string; initialValue?: string }
  | { kind: 'delete-hash-field'; key: string; field: string }
  | { kind: 'push-list-item'; key: string }
  | { kind: 'add-set-member'; key: string }
  | { kind: 'delete-set-member'; key: string; member: string }
  | { kind: 'add-zset-member'; key: string; initialMember?: string; initialScore?: number }
  | { kind: 'delete-zset-member'; key: string; member: string };

interface RedisEditorModalProps {
  isOpen: boolean;
  onClose: () => void;
  mode: RedisEditorMode;
  sessionId: string;
  onSuccess: () => void;
  readOnly?: boolean;
  environment?: 'development' | 'staging' | 'production';
  connectionName?: string;
  connectionDatabase?: string;
}

const KEY_TYPES: { value: RedisKeyType; labelKey: string }[] = [
  { value: 'string', labelKey: 'redis.typeString' },
  { value: 'hash', labelKey: 'redis.typeHash' },
  { value: 'list', labelKey: 'redis.typeList' },
  { value: 'set', labelKey: 'redis.typeSet' },
  { value: 'zset', labelKey: 'redis.typeZSet' },
];

function titleKey(mode: RedisEditorMode): string {
  switch (mode.kind) {
    case 'create-key':
      return 'redis.createKeyTitle';
    case 'delete-key':
      return 'redis.deleteKeyTitle';
    case 'edit-value':
      return 'redis.editValueTitle';
    case 'add-hash-field':
    case 'edit-hash-field':
      return 'redis.editHashFieldTitle';
    case 'delete-hash-field':
      return 'redis.deleteHashFieldTitle';
    case 'push-list-item':
      return 'redis.pushListItemTitle';
    case 'add-set-member':
      return 'redis.addSetMemberTitle';
    case 'delete-set-member':
      return 'redis.deleteSetMemberTitle';
    case 'add-zset-member':
      return 'redis.editZSetMemberTitle';
    case 'delete-zset-member':
      return 'redis.deleteZSetMemberTitle';
  }
}

function isDestructive(mode: RedisEditorMode): boolean {
  return mode.kind.startsWith('delete-');
}

export function RedisEditorModal({
  isOpen,
  onClose,
  mode,
  sessionId,
  onSuccess,
  readOnly = false,
  environment = 'development',
  connectionName,
  connectionDatabase,
}: RedisEditorModalProps) {
  const { t } = useTranslation();

  // Form state. We keep all possible fields; consumers only read the ones
  // relevant to the active mode.
  const [keyType, setKeyType] = useState<RedisKeyType>('string');
  const [key, setKey] = useState('');
  const [value, setValue] = useState('');
  const [field, setField] = useState('');
  const [member, setMember] = useState('');
  const [score, setScore] = useState<string>('0');
  const [side, setSide] = useState<ListSide>('right');
  const [ttl, setTtl] = useState<string>('');

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [pendingCommand, setPendingCommand] = useState<string | null>(null);

  const confirmationLabel =
    (connectionDatabase || connectionName || 'PROD').trim() || 'PROD';

  useEffect(() => {
    if (!isOpen) return;

    setError(null);
    setLoading(false);

    // Seed form fields from the mode's context so the UI is immediately
    // ready to act on the selected key/field/member.
    switch (mode.kind) {
      case 'create-key':
        setKeyType('string');
        setKey('');
        setValue('');
        setField('');
        setMember('');
        setScore('0');
        setSide('right');
        setTtl('');
        break;
      case 'delete-key':
        // No form fields to seed.
        break;
      case 'edit-value':
        setKeyType(mode.keyType);
        setKey(mode.key);
        setValue(mode.initialValue ?? '');
        setTtl('');
        break;
      case 'add-hash-field':
        setKey(mode.key);
        setField('');
        setValue('');
        break;
      case 'edit-hash-field':
        setKey(mode.key);
        setField(mode.field);
        setValue(mode.initialValue ?? '');
        break;
      case 'delete-hash-field':
        setKey(mode.key);
        setField(mode.field);
        break;
      case 'push-list-item':
        setKey(mode.key);
        setValue('');
        setSide('right');
        break;
      case 'add-set-member':
        setKey(mode.key);
        setMember('');
        break;
      case 'delete-set-member':
        setKey(mode.key);
        setMember(mode.member);
        break;
      case 'add-zset-member':
        setKey(mode.key);
        setMember(mode.initialMember ?? '');
        setScore(mode.initialScore?.toString() ?? '0');
        break;
      case 'delete-zset-member':
        setKey(mode.key);
        setMember(mode.member);
        break;
    }
  }, [isOpen, mode]);

  const description = useMemo(() => {
    if (mode.kind === 'delete-key') {
      return t('redis.deleteKeyDescription', {
        count: mode.keys.length,
        key: mode.keys[0],
      });
    }
    if (isDestructive(mode)) {
      return t('redis.deleteGenericDescription');
    }
    return t('environment.mutationConfirmGeneric');
  }, [mode, t]);

  function validateAndBuild(): string {
    switch (mode.kind) {
      case 'create-key':
      case 'edit-value': {
        if (!key.trim()) throw new Error(t('redis.errorKeyRequired'));
        if (mode.kind === 'create-key') {
          switch (keyType) {
            case 'string':
              return buildSetString({
                key,
                value,
                ttlSeconds: ttl ? Number(ttl) : undefined,
              });
            case 'hash':
              if (!field.trim()) throw new Error(t('redis.errorFieldRequired'));
              return buildSetHashField({ key, field, value });
            case 'list':
              return buildPushListItem({ key, value, side });
            case 'set':
              if (!member.trim()) throw new Error(t('redis.errorMemberRequired'));
              return buildAddSetMember({ key, member });
            case 'zset': {
              if (!member.trim()) throw new Error(t('redis.errorMemberRequired'));
              const n = Number(score);
              if (!Number.isFinite(n)) throw new Error(t('redis.errorScoreInvalid'));
              return buildSetZSetMember({ key, member, score: n });
            }
          }
        }
        return buildSetString({
          key,
          value,
          ttlSeconds: ttl ? Number(ttl) : undefined,
        });
      }
      case 'delete-key':
        return buildDeleteKeys(mode.keys);
      case 'add-hash-field':
      case 'edit-hash-field':
        if (!field.trim()) throw new Error(t('redis.errorFieldRequired'));
        return buildSetHashField({ key, field, value });
      case 'delete-hash-field':
        return buildDeleteHashField({ key, field });
      case 'push-list-item':
        return buildPushListItem({ key, value, side });
      case 'add-set-member':
        if (!member.trim()) throw new Error(t('redis.errorMemberRequired'));
        return buildAddSetMember({ key, member });
      case 'delete-set-member':
        return buildRemoveSetMember({ key, member });
      case 'add-zset-member': {
        if (!member.trim()) throw new Error(t('redis.errorMemberRequired'));
        const n = Number(score);
        if (!Number.isFinite(n)) throw new Error(t('redis.errorScoreInvalid'));
        return buildSetZSetMember({ key, member, score: n });
      }
      case 'delete-zset-member':
        return buildRemoveZSetMember({ key, member });
    }
  }

  async function handleSubmit() {
    if (readOnly) {
      toast.error(t('environment.blocked'));
      return;
    }

    setError(null);

    let command: string;
    try {
      command = validateAndBuild();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      return;
    }

    if (environment !== 'development' || isDestructive(mode)) {
      setPendingCommand(command);
      setConfirmOpen(true);
      return;
    }

    await runCommand(command, false);
  }

  async function runCommand(command: string, acknowledgedDangerous: boolean) {
    setLoading(true);
    setError(null);
    try {
      const res = await executeQuery(sessionId, command, {
        acknowledgedDangerous,
      });
      if (res.success) {
        toast.success(t('redis.mutationSuccess'));
        onSuccess();
        onClose();
      } else {
        setError(res.error ?? t('common.unknownError'));
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : t('common.unknownError'));
    } finally {
      setLoading(false);
    }
  }

  return (
    <>
      <Dialog open={isOpen} onOpenChange={open => !open && !loading && onClose()}>
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle>{t(titleKey(mode))}</DialogTitle>
          </DialogHeader>

          <div className="space-y-4">
            {mode.kind === 'create-key' && (
              <div className="space-y-2">
                <Label htmlFor="redis-keytype">{t('redis.keyType')}</Label>
                <Select
                  value={keyType}
                  onValueChange={v => setKeyType(v as RedisKeyType)}
                >
                  <SelectTrigger id="redis-keytype" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {KEY_TYPES.map(({ value: v, labelKey }) => (
                      <SelectItem key={v} value={v}>
                        {t(labelKey)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}

            {(mode.kind === 'create-key' || mode.kind === 'edit-value') && (
              <div className="space-y-2">
                <Label htmlFor="redis-key">{t('redis.keyName')}</Label>
                <Input
                  id="redis-key"
                  value={key}
                  onChange={e => setKey(e.target.value)}
                  disabled={mode.kind === 'edit-value'}
                  placeholder="user:42"
                />
              </div>
            )}

            {mode.kind === 'delete-key' && (
              <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm">
                <div className="font-medium">
                  {t('redis.deleteKeysSummary', { count: mode.keys.length })}
                </div>
                <ul className="mt-2 max-h-24 space-y-0.5 overflow-y-auto font-mono text-xs">
                  {mode.keys.slice(0, 20).map(k => (
                    <li key={k}>{k}</li>
                  ))}
                  {mode.keys.length > 20 && (
                    <li className="text-muted-foreground">…</li>
                  )}
                </ul>
              </div>
            )}

            {(mode.kind === 'add-hash-field' ||
              mode.kind === 'edit-hash-field' ||
              mode.kind === 'delete-hash-field') && (
              <>
                <div className="space-y-2">
                  <Label htmlFor="redis-hash-key">{t('redis.keyName')}</Label>
                  <Input id="redis-hash-key" value={key} disabled />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="redis-hash-field">{t('redis.hashField')}</Label>
                  <Input
                    id="redis-hash-field"
                    value={field}
                    onChange={e => setField(e.target.value)}
                    disabled={
                      mode.kind === 'edit-hash-field' || mode.kind === 'delete-hash-field'
                    }
                  />
                </div>
              </>
            )}

            {(mode.kind === 'add-set-member' ||
              mode.kind === 'delete-set-member' ||
              mode.kind === 'add-zset-member' ||
              mode.kind === 'delete-zset-member') && (
              <>
                <div className="space-y-2">
                  <Label htmlFor="redis-sm-key">{t('redis.keyName')}</Label>
                  <Input id="redis-sm-key" value={key} disabled />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="redis-member">{t('redis.member')}</Label>
                  <Input
                    id="redis-member"
                    value={member}
                    onChange={e => setMember(e.target.value)}
                    disabled={
                      mode.kind === 'delete-set-member' || mode.kind === 'delete-zset-member'
                    }
                  />
                </div>
              </>
            )}

            {mode.kind === 'add-zset-member' && (
              <div className="space-y-2">
                <Label htmlFor="redis-score">{t('redis.score')}</Label>
                <Input
                  id="redis-score"
                  type="number"
                  step="any"
                  value={score}
                  onChange={e => setScore(e.target.value)}
                />
              </div>
            )}

            {(mode.kind === 'push-list-item' ||
              (mode.kind === 'create-key' && keyType === 'list')) && (
              <div className="space-y-2">
                <Label htmlFor="redis-list-side">{t('redis.listSide')}</Label>
                <Select value={side} onValueChange={v => setSide(v as ListSide)}>
                  <SelectTrigger id="redis-list-side" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="left">{t('redis.listLeft')}</SelectItem>
                    <SelectItem value="right">{t('redis.listRight')}</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            )}

            {/* Value input is relevant for most creation/edit modes. */}
            {(mode.kind === 'create-key' ||
              mode.kind === 'edit-value' ||
              mode.kind === 'add-hash-field' ||
              mode.kind === 'edit-hash-field' ||
              mode.kind === 'push-list-item') &&
              // For create-key of set/zset the value is held in the `member`
              // field, not `value` — hide the generic value input.
              !(mode.kind === 'create-key' && (keyType === 'set' || keyType === 'zset')) && (
                <div className="space-y-2">
                  <Label htmlFor="redis-value">{t('redis.value')}</Label>
                  <Input
                    id="redis-value"
                    value={value}
                    onChange={e => setValue(e.target.value)}
                  />
                </div>
              )}

            {mode.kind === 'create-key' && (keyType === 'set' || keyType === 'zset') && (
              <div className="space-y-2">
                <Label htmlFor="redis-initial-member">{t('redis.member')}</Label>
                <Input
                  id="redis-initial-member"
                  value={member}
                  onChange={e => setMember(e.target.value)}
                />
                {keyType === 'zset' && (
                  <>
                    <Label htmlFor="redis-initial-score" className="mt-2">
                      {t('redis.score')}
                    </Label>
                    <Input
                      id="redis-initial-score"
                      type="number"
                      step="any"
                      value={score}
                      onChange={e => setScore(e.target.value)}
                    />
                  </>
                )}
              </div>
            )}

            {(mode.kind === 'create-key' && keyType === 'string') ||
            mode.kind === 'edit-value' ? (
              <div className="space-y-2">
                <Label htmlFor="redis-ttl">{t('redis.ttlSeconds')}</Label>
                <Input
                  id="redis-ttl"
                  type="number"
                  min={0}
                  value={ttl}
                  onChange={e => setTtl(e.target.value)}
                  placeholder={t('redis.ttlPlaceholder')}
                />
              </div>
            ) : null}

            {error && (
              <div className="rounded-md border border-destructive/20 bg-destructive/10 p-2 text-sm text-destructive">
                {error}
              </div>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={onClose} disabled={loading}>
              {t('common.cancel')}
            </Button>
            <Button
              onClick={handleSubmit}
              disabled={loading || readOnly}
              variant={isDestructive(mode) ? 'destructive' : 'default'}
            >
              {loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {isDestructive(mode) ? t('common.delete') : t('common.save')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <DangerConfirmDialog
        open={confirmOpen}
        onOpenChange={open => {
          setConfirmOpen(open);
          if (!open) setPendingCommand(null);
        }}
        title={t('environment.mutationConfirmTitle')}
        description={description}
        confirmationLabel={environment === 'production' ? confirmationLabel : undefined}
        confirmLabel={t('common.confirm')}
        loading={loading}
        onConfirm={() => {
          const cmd = pendingCommand;
          setPendingCommand(null);
          if (cmd) void runCommand(cmd, true);
        }}
      />
    </>
  );
}
