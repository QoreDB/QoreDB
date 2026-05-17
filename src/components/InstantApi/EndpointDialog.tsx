// SPDX-License-Identifier: BUSL-1.1

import { Plus, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
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
import { Textarea } from '@/components/ui/textarea';
import {
  type CreateEndpointResponse,
  createEndpoint,
  type EndpointParam,
  type EndpointParamType,
  type QueryShape,
} from '@/lib/instantApi';
import { getWorkspaceProjectId, listSavedConnections, type SavedConnection } from '@/lib/tauri';

interface Props {
  open: boolean;
  onClose: () => void;
  /** Called after successful create — caller shows the one-shot token. */
  onCreated: (response: CreateEndpointResponse) => void;
}

/**
 * Local-only id stamped on each param row so React keys remain stable when
 * users insert/delete rows in the middle of the list. Stripped before the
 * payload is sent to the backend.
 */
interface ParamRow extends EndpointParam {
  _key: string;
}

let paramKeySeq = 0;
function nextParamKey(): string {
  paramKeySeq += 1;
  return `p${paramKeySeq}`;
}

const PARAM_TYPES: EndpointParamType[] = ['string', 'integer', 'float', 'bool'];
const SHAPES: QueryShape[] = ['rows', 'object'];
const NAME_RE = /^[A-Za-z0-9_-]{1,64}$/;

export function EndpointDialog({ open, onClose, onCreated }: Props) {
  const { t } = useTranslation();
  const [name, setName] = useState('');
  const [connectionId, setConnectionId] = useState<string>('');
  const [querySource, setQuerySource] = useState('');
  const [shape, setShape] = useState<QueryShape>('rows');
  const [pageSize, setPageSize] = useState(100);
  const [params, setParams] = useState<ParamRow[]>([]);
  const [connections, setConnections] = useState<SavedConnection[]>([]);
  const [submitting, setSubmitting] = useState(false);

  const loadConnections = useCallback(async () => {
    try {
      const projectId = await getWorkspaceProjectId();
      const list = await listSavedConnections(projectId);
      setConnections(list);
      setConnectionId(current => current || list[0]?.id || '');
    } catch (e) {
      toast.error(t('instantApi.endpoint.loadConnectionsFailed'), {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  }, [t]);

  useEffect(() => {
    if (!open) return;
    setName('');
    setQuerySource('');
    setShape('rows');
    setPageSize(100);
    setParams([]);
    void loadConnections();
  }, [open, loadConnections]);

  function addParam() {
    setParams(prev => [
      ...prev,
      { _key: nextParamKey(), name: '', type: 'string', required: true, default: null },
    ]);
  }

  function updateParam(key: string, patch: Partial<EndpointParam>) {
    setParams(prev => prev.map(p => (p._key === key ? { ...p, ...patch } : p)));
  }

  function removeParam(key: string) {
    setParams(prev => prev.filter(p => p._key !== key));
  }

  function validate(): string | null {
    if (!NAME_RE.test(name)) return t('instantApi.endpoint.invalidName');
    if (!connectionId) return t('instantApi.endpoint.missingConnection');
    if (querySource.trim().length === 0) return t('instantApi.endpoint.missingQuery');
    if (pageSize < 1 || pageSize > 10000) return t('instantApi.endpoint.invalidPageSize');
    for (const p of params) {
      if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(p.name)) {
        return t('instantApi.endpoint.invalidParamName', { name: p.name || '?' });
      }
      if (!querySource.includes(`{{${p.name}}}`)) {
        return t('instantApi.endpoint.paramNotInQuery', { name: p.name });
      }
    }
    return null;
  }

  async function handleSubmit() {
    const error = validate();
    if (error) {
      toast.error(error);
      return;
    }
    setSubmitting(true);
    try {
      const response = await createEndpoint({
        name,
        connectionId,
        querySource,
        shape,
        pageSize,
        params: params.map(({ _key, ...rest }) => ({
          ...rest,
          default: rest.default ?? null,
        })),
      });
      onCreated(response);
      onClose();
    } catch (e) {
      toast.error(t('instantApi.endpoint.createFailed'), {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={v => !v && onClose()}>
      <DialogContent className="max-w-2xl max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('instantApi.endpoint.newTitle')}</DialogTitle>
          <DialogDescription>{t('instantApi.endpoint.newDescription')}</DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 py-2 pr-1">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <Label htmlFor="ep-name">{t('instantApi.endpoint.nameLabel')}</Label>
              <Input
                id="ep-name"
                value={name}
                onChange={e => setName(e.target.value)}
                placeholder="orders_top"
                spellCheck={false}
              />
              <p className="text-[11px] text-muted-foreground mt-1">
                {t('instantApi.endpoint.nameHint')}
              </p>
            </div>
            <div>
              <Label htmlFor="ep-conn">{t('instantApi.endpoint.connectionLabel')}</Label>
              <Select value={connectionId} onValueChange={setConnectionId}>
                <SelectTrigger id="ep-conn">
                  <SelectValue placeholder={t('instantApi.endpoint.selectConnection')} />
                </SelectTrigger>
                <SelectContent>
                  {connections.map(c => (
                    <SelectItem key={c.id} value={c.id}>
                      {c.name} ({c.driver})
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>

          <div>
            <Label htmlFor="ep-query">{t('instantApi.endpoint.queryLabel')}</Label>
            <Textarea
              id="ep-query"
              value={querySource}
              onChange={e => setQuerySource(e.target.value)}
              placeholder="SELECT * FROM orders WHERE country = {{country}} LIMIT 50"
              rows={6}
              spellCheck={false}
              className="font-mono text-xs"
            />
            <p className="text-[11px] text-muted-foreground mt-1">
              {t('instantApi.endpoint.queryHint')}
            </p>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <Label htmlFor="ep-shape">{t('instantApi.endpoint.shapeLabel')}</Label>
              <Select value={shape} onValueChange={v => setShape(v as QueryShape)}>
                <SelectTrigger id="ep-shape">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {SHAPES.map(s => (
                    <SelectItem key={s} value={s}>
                      {t(`instantApi.endpoint.shape.${s}`)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div>
              <Label htmlFor="ep-pagesize">{t('instantApi.endpoint.pageSizeLabel')}</Label>
              <Input
                id="ep-pagesize"
                type="number"
                value={pageSize}
                onChange={e => setPageSize(Number.parseInt(e.target.value, 10) || 100)}
                min={1}
                max={10000}
              />
            </div>
          </div>

          <div>
            <div className="flex items-center justify-between mb-2">
              <Label>{t('instantApi.endpoint.paramsLabel')}</Label>
              <Button variant="ghost" size="sm" onClick={addParam}>
                <Plus size={13} />
                {t('instantApi.endpoint.addParam')}
              </Button>
            </div>
            {params.length === 0 ? (
              <p className="text-[11px] text-muted-foreground italic">
                {t('instantApi.endpoint.noParams')}
              </p>
            ) : (
              <div className="space-y-2">
                {params.map(p => (
                  <ParamRowEditor
                    key={p._key}
                    param={p}
                    onChange={patch => updateParam(p._key, patch)}
                    onRemove={() => removeParam(p._key)}
                  />
                ))}
              </div>
            )}
          </div>
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={onClose} disabled={submitting}>
            {t('instantApi.endpoint.cancel')}
          </Button>
          <Button onClick={handleSubmit} disabled={submitting}>
            {submitting ? t('instantApi.endpoint.creating') : t('instantApi.endpoint.create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

interface ParamRowEditorProps {
  param: EndpointParam;
  onChange: (patch: Partial<EndpointParam>) => void;
  onRemove: () => void;
}

function ParamRowEditor({ param, onChange, onRemove }: ParamRowEditorProps) {
  const { t } = useTranslation();
  return (
    <div className="grid grid-cols-[1fr_120px_100px_32px] gap-2 items-start">
      <Input
        value={param.name}
        onChange={e => onChange({ name: e.target.value })}
        placeholder={t('instantApi.endpoint.paramName')}
        spellCheck={false}
        className="text-xs font-mono"
      />
      <Select value={param.type} onValueChange={v => onChange({ type: v as EndpointParamType })}>
        <SelectTrigger size="sm">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {PARAM_TYPES.map(p => (
            <SelectItem key={p} value={p}>
              {p}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Input
        value={param.default ?? ''}
        onChange={e => onChange({ default: e.target.value || null })}
        placeholder={t('instantApi.endpoint.paramDefault')}
        className="text-xs"
      />
      <Button variant="ghost" size="sm" onClick={onRemove}>
        <Trash2 size={13} />
      </Button>
    </div>
  );
}
