// SPDX-License-Identifier: Apache-2.0

import { Lock, Shield } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { isDocumentDatabase } from '@/lib/connection/driverCapabilities';
import {
  DEFAULT_PORTS,
  Driver,
  getDriverMetadata,
  isKeyValueDriver,
} from '@/lib/connection/drivers';
import { ENVIRONMENT_CONFIG } from '@/lib/environment';
import type { MssqlAuthMode, SearchAuthMode, SnowflakeAuthMode } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { Field } from './Field';
import { FileSection } from './FileSection';
import { PasswordInput } from './PasswordInput';
import type { ConnectionFormData } from './types';

interface BasicSectionProps {
  formData: ConnectionFormData;
  onChange: (field: keyof ConnectionFormData, value: string | number | boolean) => void;
  /** Hide host/port/username/password fields (used when URL mode provides these) */
  hideConnectionFields?: boolean;
}

export function BasicSection({
  formData,
  onChange,
  hideConnectionFields = false,
}: BasicSectionProps) {
  const { t } = useTranslation();

  const isFileBased = formData.driver === Driver.Sqlite || formData.driver === Driver.Duckdb;
  const usernameRequired =
    !isDocumentDatabase(formData.driver) &&
    !isKeyValueDriver(formData.driver) &&
    formData.driver !== Driver.Cassandra &&
    formData.driver !== Driver.ScyllaDb;
  const isSqlServer = [Driver.SqlServer, Driver.AzureSql, Driver.Synapse].includes(formData.driver);
  const isClickhouse = formData.driver === Driver.Clickhouse;
  const isRedis = isKeyValueDriver(formData.driver);
  const isSearch =
    formData.driver === Driver.Elasticsearch || formData.driver === Driver.OpenSearch;
  const isSnowflake = formData.driver === Driver.Snowflake;
  const isBigQuery = formData.driver === Driver.BigQuery;
  const driverMeta = getDriverMetadata(formData.driver);
  const isNtlm = isSqlServer && formData.mssqlAuthMode === 'windows_ntlm';
  const isIntegrated = isSqlServer && formData.mssqlAuthMode === 'windows_integrated';
  const authModes: { value: MssqlAuthMode; label: string }[] = [
    { value: 'sql_password', label: t('connection.mssql.authSql') },
    { value: 'windows_ntlm', label: t('connection.mssql.authNtlm') },
    { value: 'windows_integrated', label: t('connection.mssql.authIntegrated') },
  ];
  const searchAuthModes: { value: SearchAuthMode; label: string }[] = [
    { value: 'none', label: t('connection.search.authNone') },
    { value: 'basic', label: t('connection.search.authBasic') },
    { value: 'api_key', label: t('connection.search.authApiKey') },
    { value: 'bearer', label: t('connection.search.authBearer') },
  ];
  const snowflakeAuthModes: { value: SnowflakeAuthMode; label: string }[] = [
    { value: 'key_pair', label: t('connection.snowflake.authKeyPair') },
    { value: 'token', label: t('connection.snowflake.authToken') },
  ];
  const searchSecretLabel =
    formData.searchAuthMode === 'api_key'
      ? t('connection.search.apiKey')
      : formData.searchAuthMode === 'bearer'
        ? t('connection.search.bearerToken')
        : t('connection.password');

  return (
    <div className="rounded-md border border-border bg-background p-4 space-y-4">
      <Field label={t('connection.connectionName')}>
        <Input
          placeholder="My Database"
          value={formData.name}
          onChange={e => onChange('name', e.target.value)}
        />
      </Field>

      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label className="flex items-center gap-2">
            <Shield size={14} className="text-muted-foreground" />
            {t('environment.label')}
          </Label>
          <div className="flex gap-2">
            {(['development', 'staging', 'production'] as const).map(env => {
              const config = ENVIRONMENT_CONFIG[env];
              const isSelected = formData.environment === env;
              return (
                <Button
                  key={env}
                  type="button"
                  variant="ghost"
                  size="sm"
                  className={cn(
                    'h-auto flex-1 px-3 py-2 rounded-md text-xs font-semibold border-2 transition-all',
                    isSelected
                      ? 'border-transparent shadow-sm'
                      : 'border-border bg-background hover:bg-muted text-muted-foreground'
                  )}
                  style={
                    isSelected
                      ? {
                          backgroundColor: config.bgSoft,
                          color: config.color,
                          border: `2px solid ${config.color}`,
                        }
                      : undefined
                  }
                  onClick={() => onChange('environment', env)}
                >
                  {config.labelShort}
                </Button>
              );
            })}
          </div>
        </div>

        <div className="space-y-2">
          <Label className="flex items-center gap-2">
            <Lock size={14} className="text-muted-foreground" />
            {t('environment.readOnly')}
          </Label>
          <div className="flex items-center justify-between rounded-md border border-border bg-background px-3 py-2">
            <span
              className={cn(
                'text-sm',
                formData.readOnly ? 'text-warning' : 'text-muted-foreground'
              )}
            >
              {formData.readOnly ? t('common.enabled') : t('common.disabled')}
            </span>
            <Switch
              checked={formData.readOnly}
              onCheckedChange={checked => onChange('readOnly', checked)}
            />
          </div>
        </div>
      </div>

      {/* File-based connection for SQLite */}
      {isFileBased && !hideConnectionFields && (
        <FileSection formData={formData} onChange={onChange} />
      )}

      {isBigQuery && !hideConnectionFields && (
        <div className="space-y-4">
          <Field label={t('connection.project')} hint={t('connection.bigquery.projectHint')}>
            <Input
              placeholder="my-gcp-project"
              value={formData.database}
              onChange={e => onChange('database', e.target.value)}
              spellCheck={false}
            />
          </Field>
          <Field
            label={t('connection.bigquery.serviceAccount')}
            hint={t('connection.bigquery.serviceAccountHint')}
            required
          >
            <Textarea
              placeholder={'{ "type": "service_account", "project_id": "…", … }'}
              value={formData.password}
              onChange={e => onChange('password', e.target.value)}
              spellCheck={false}
              rows={5}
              className="font-mono text-xs"
            />
          </Field>
          <div className="grid grid-cols-2 gap-4">
            <Field
              label={t('connection.bigquery.location')}
              hint={t('connection.bigquery.locationHint')}
            >
              <Input
                placeholder="EU"
                value={formData.bigqueryLocation}
                onChange={e => onChange('bigqueryLocation', e.target.value)}
                spellCheck={false}
              />
            </Field>
            <Field
              label={t('connection.bigquery.billingProject')}
              hint={t('connection.bigquery.billingProjectHint')}
            >
              <Input
                placeholder="my-billing-project"
                value={formData.bigqueryBillingProject}
                onChange={e => onChange('bigqueryBillingProject', e.target.value)}
                spellCheck={false}
              />
            </Field>
          </div>
        </div>
      )}

      {/* Connection fields - hidden when URL mode provides them or for file-based drivers */}
      {!hideConnectionFields && !isFileBased && !isBigQuery && (
        <>
          <div className="grid grid-cols-3 gap-4">
            <Field
              label={isSnowflake ? t('connection.snowflake.account') : t('connection.host')}
              hint={isSnowflake ? t('connection.snowflake.accountHint') : undefined}
              required
              className={isSnowflake ? 'col-span-3' : 'col-span-2'}
            >
              <Input
                placeholder={isSnowflake ? 'myorg-myaccount' : 'localhost'}
                value={formData.host}
                onChange={e => onChange('host', e.target.value)}
                spellCheck={false}
              />
            </Field>
            {!isSnowflake && (
              <Field label={t('connection.port')}>
                <Input
                  type="number"
                  min={1}
                  max={65535}
                  placeholder={String(DEFAULT_PORTS[formData.driver])}
                  value={formData.port || ''}
                  onChange={e =>
                    onChange('port', e.target.value === '' ? 0 : parseInt(e.target.value, 10) || 0)
                  }
                />
              </Field>
            )}
          </div>

          <Field label={t(driverMeta.databaseFieldLabel)}>
            <Input
              type={isRedis ? 'number' : 'text'}
              min={isRedis ? 0 : undefined}
              max={isRedis ? 15 : undefined}
              placeholder={formData.driver === Driver.Postgres ? 'postgres' : ''}
              value={formData.database}
              onChange={e => onChange('database', e.target.value)}
            />
          </Field>

          {isSnowflake && (
            <div className="grid grid-cols-2 gap-4">
              <Field
                label={t('connection.snowflake.warehouse')}
                hint={t('connection.snowflake.warehouseHint')}
              >
                <Input
                  placeholder="COMPUTE_WH"
                  value={formData.snowflakeWarehouse}
                  onChange={e => onChange('snowflakeWarehouse', e.target.value)}
                  spellCheck={false}
                />
              </Field>
              <Field label={t('connection.snowflake.role')}>
                <Input
                  placeholder="SYSADMIN"
                  value={formData.snowflakeRole}
                  onChange={e => onChange('snowflakeRole', e.target.value)}
                  spellCheck={false}
                />
              </Field>
            </div>
          )}

          {isSnowflake && (
            <div className="space-y-2">
              <Label>{t('connection.snowflake.authMode')}</Label>
              <div className="flex gap-2">
                {snowflakeAuthModes.map(({ value, label }) => {
                  const isSelected = formData.snowflakeAuthMode === value;
                  return (
                    <Button
                      key={value}
                      type="button"
                      variant="ghost"
                      size="sm"
                      className={cn(
                        'h-auto flex-1 px-3 py-2 rounded-md text-xs font-semibold border-2 transition-all',
                        isSelected
                          ? 'border-primary bg-primary/10 text-primary'
                          : 'border-border bg-background hover:bg-muted text-muted-foreground'
                      )}
                      onClick={() => onChange('snowflakeAuthMode', value)}
                    >
                      {label}
                    </Button>
                  );
                })}
              </div>
            </div>
          )}

          {isSqlServer && (
            <div className="space-y-2">
              <Label>{t('connection.mssql.authMode')}</Label>
              <div className="flex gap-2">
                {authModes.map(({ value, label }) => {
                  const isSelected = formData.mssqlAuthMode === value;
                  return (
                    <Button
                      key={value}
                      type="button"
                      variant="ghost"
                      size="sm"
                      className={cn(
                        'h-auto flex-1 px-3 py-2 rounded-md text-xs font-semibold border-2 transition-all',
                        isSelected
                          ? 'border-primary bg-primary/10 text-primary'
                          : 'border-border bg-background hover:bg-muted text-muted-foreground'
                      )}
                      onClick={() => onChange('mssqlAuthMode', value)}
                    >
                      {label}
                    </Button>
                  );
                })}
              </div>
              {isNtlm && (
                <p className="text-xs text-muted-foreground">{t('connection.mssql.ntlmHint')}</p>
              )}
              {isIntegrated && (
                <p className="text-xs text-muted-foreground">
                  {t('connection.mssql.integratedHint')}
                </p>
              )}
            </div>
          )}

          {isClickhouse && (
            <Field
              label={t('connection.clickhouse.cluster')}
              hint={t('connection.clickhouse.clusterHint')}
            >
              <Input
                placeholder={t('connection.clickhouse.clusterPlaceholder')}
                value={formData.clickhouseCluster}
                onChange={e => onChange('clickhouseCluster', e.target.value)}
                spellCheck={false}
              />
            </Field>
          )}

          {isSearch && (
            <div className="space-y-2">
              <Label>{t('connection.search.authMode')}</Label>
              <div className="flex gap-2">
                {searchAuthModes.map(({ value, label }) => {
                  const isSelected = formData.searchAuthMode === value;
                  return (
                    <Button
                      key={value}
                      type="button"
                      variant="ghost"
                      size="sm"
                      className={cn(
                        'h-auto flex-1 px-3 py-2 rounded-md text-xs font-semibold border-2 transition-all',
                        isSelected
                          ? 'border-primary bg-primary/10 text-primary'
                          : 'border-border bg-background hover:bg-muted text-muted-foreground'
                      )}
                      onClick={() => onChange('searchAuthMode', value)}
                    >
                      {label}
                    </Button>
                  );
                })}
              </div>
              {formData.searchAuthMode !== 'none' && !formData.ssl && (
                <p className="text-xs text-warning">{t('connection.search.tlsWarning')}</p>
              )}
            </div>
          )}

          {(isSearch || isDocumentDatabase(formData.driver)) && formData.ssl && (
            <Field
              label={t('connection.search.caCert')}
              hint={
                formData.driver === Driver.DocumentDb
                  ? t('connection.documentdb.caCertHint')
                  : t('connection.search.caCertHint')
              }
            >
              <Input
                placeholder={
                  formData.driver === Driver.DocumentDb
                    ? '/path/to/global-bundle.pem'
                    : '/etc/ssl/certs/ca.pem'
                }
                value={formData.sslCaCert}
                onChange={e => onChange('sslCaCert', e.target.value)}
                spellCheck={false}
              />
            </Field>
          )}

          {isSnowflake && formData.snowflakeAuthMode === 'token' && (
            <Field label={t('connection.snowflake.token')} required>
              <PasswordInput
                placeholder="••••••••"
                value={formData.password}
                onChange={e => onChange('password', e.target.value)}
              />
            </Field>
          )}

          {isSnowflake && formData.snowflakeAuthMode === 'key_pair' && (
            <div className="space-y-4">
              <Field label={t('connection.username')} required>
                <Input
                  placeholder="ALICE"
                  value={formData.username}
                  onChange={e => onChange('username', e.target.value)}
                  spellCheck={false}
                />
              </Field>
              <Field
                label={t('connection.snowflake.privateKey')}
                hint={t('connection.snowflake.privateKeyHint')}
                required
              >
                <Textarea
                  placeholder={'-----BEGIN PRIVATE KEY-----\n…\n-----END PRIVATE KEY-----'}
                  value={formData.password}
                  onChange={e => onChange('password', e.target.value)}
                  spellCheck={false}
                  rows={5}
                  className="font-mono text-xs"
                />
              </Field>
            </div>
          )}

          {!isSnowflake &&
            (isSearch
              ? formData.searchAuthMode !== 'none' && (
                  <div className="grid grid-cols-2 gap-4">
                    {formData.searchAuthMode === 'basic' && (
                      <Field label={t('connection.username')} required>
                        <Input
                          placeholder="elastic"
                          value={formData.username}
                          onChange={e => onChange('username', e.target.value)}
                        />
                      </Field>
                    )}
                    <Field
                      label={searchSecretLabel}
                      className={formData.searchAuthMode === 'basic' ? undefined : 'col-span-2'}
                    >
                      <PasswordInput
                        placeholder="••••••••"
                        value={formData.password}
                        onChange={e => onChange('password', e.target.value)}
                      />
                    </Field>
                  </div>
                )
              : !isIntegrated && (
                  <div className="grid grid-cols-2 gap-4">
                    <Field label={t('connection.username')} required={usernameRequired}>
                      <Input
                        placeholder={
                          isNtlm ? t('connection.mssql.ntlmUsernamePlaceholder') : 'user'
                        }
                        value={formData.username}
                        onChange={e => onChange('username', e.target.value)}
                      />
                    </Field>
                    <Field label={t('connection.password')}>
                      <PasswordInput
                        placeholder="••••••••"
                        value={formData.password}
                        onChange={e => onChange('password', e.target.value)}
                      />
                    </Field>
                  </div>
                ))}
        </>
      )}
    </div>
  );
}
