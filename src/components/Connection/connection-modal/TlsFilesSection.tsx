// SPDX-License-Identifier: Apache-2.0

import { open } from '@tauri-apps/plugin-dialog';
import { FolderOpen } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Field } from './Field';
import type { ConnectionFormData } from './types';

type TlsFileField = 'sslCaCert' | 'sslClientCert' | 'sslClientKey';

const SMALL_LABEL = 'text-xs text-muted-foreground';

const FILES: { field: TlsFileField; label: string; hint: string; placeholder: string }[] = [
  {
    field: 'sslCaCert',
    label: 'connection.tls.caCert',
    hint: 'connection.tls.caCertHint',
    placeholder: '/path/to/ca.crt',
  },
  {
    field: 'sslClientCert',
    label: 'connection.tls.clientCert',
    hint: 'connection.tls.clientCertHint',
    placeholder: '/path/to/client.crt',
  },
  {
    field: 'sslClientKey',
    label: 'connection.tls.clientKey',
    hint: 'connection.tls.clientKeyHint',
    placeholder: '/path/to/client.key',
  },
];

function PathInput(props: {
  id?: string;
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
}) {
  const { t } = useTranslation();

  async function handleBrowse() {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: 'PEM', extensions: ['pem', 'crt', 'cer', 'key'] },
          { name: 'All Files', extensions: ['*'] },
        ],
      });
      if (selected && typeof selected === 'string') props.onChange(selected);
    } catch (err) {
      console.error('Failed to open file dialog:', err);
    }
  }

  return (
    <div className="flex gap-2">
      <Input
        id={props.id}
        placeholder={props.placeholder}
        value={props.value}
        onChange={e => props.onChange(e.target.value)}
        spellCheck={false}
      />
      <Button
        type="button"
        variant="outline"
        size="icon"
        onClick={handleBrowse}
        title={t('connection.browseFile')}
      >
        <FolderOpen size={16} />
      </Button>
    </div>
  );
}

export function TlsFilesSection(props: {
  formData: ConnectionFormData;
  onChange: (field: keyof ConnectionFormData, value: string) => void;
}) {
  const { formData, onChange } = props;
  const { t } = useTranslation();

  return (
    <div className="space-y-3 rounded-md border border-border p-3">
      <p className="text-sm font-medium">{t('connection.tls.title')}</p>
      {FILES.map(({ field, label, hint, placeholder }) => (
        <Field key={field} label={t(label)} hint={t(hint)} labelClassName={SMALL_LABEL}>
          <PathInput
            value={formData[field]}
            placeholder={placeholder}
            onChange={value => onChange(field, value)}
          />
        </Field>
      ))}
    </div>
  );
}
