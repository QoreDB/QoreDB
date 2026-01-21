import { useTranslation } from "react-i18next";
import { Lock, Shield } from "lucide-react";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import { ENVIRONMENT_CONFIG } from "@/lib/environment";

import type { ConnectionFormData } from "./types";

export function BasicSection(props: {
	formData: ConnectionFormData;
	onChange: (field: keyof ConnectionFormData, value: string | number | boolean) => void;
}) {
	const { formData, onChange } = props;
	const { t } = useTranslation();

	return (
    <div className="rounded-md border border-border bg-background p-4 space-y-4">
      <div className="space-y-2">
        <Label>{t('connection.connectionName')}</Label>
        <Input
          placeholder="My Database"
          value={formData.name}
          onChange={e => onChange('name', e.target.value)}
        />
      </div>

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
                <button
                  key={env}
                  type="button"
                  className={cn(
                    'flex-1 px-3 py-2 rounded-md text-xs font-medium border transition-all',
                    isSelected ? 'border-transparent' : 'border-border bg-background hover:bg-muted'
                  )}
                  style={
                    isSelected
                      ? {
                          backgroundColor: config.bgSoft,
                          color: config.color,
                          borderColor: config.color,
                        }
                      : undefined
                  }
                  onClick={() => onChange('environment', env)}
                >
                  {config.labelShort}
                </button>
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

      <div className="grid grid-cols-3 gap-4">
        <div className="col-span-2 space-y-2">
          <Label>
            {t('connection.host')} <span className="text-error">*</span>
          </Label>
          <Input
            placeholder="localhost"
            value={formData.host}
            onChange={e => onChange('host', e.target.value)}
          />
        </div>
        <div className="space-y-2">
          <Label>{t('connection.port')}</Label>
          <Input
            type="number"
            value={formData.port}
            onChange={e => onChange('port', parseInt(e.target.value) || 0)}
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label>
            {t('connection.username')} <span className="text-error">*</span>
          </Label>
          <Input
            placeholder="user"
            value={formData.username}
            onChange={e => onChange('username', e.target.value)}
          />
        </div>
        <div className="space-y-2">
          <Label>
            {t('connection.password')} <span className="text-error">*</span>
          </Label>
          <Input
            type="password"
            placeholder="••••••••"
            value={formData.password}
            onChange={e => onChange('password', e.target.value)}
          />
        </div>
      </div>
    </div>
  );
}
