// SPDX-License-Identifier: Apache-2.0

import type { ShareProviderConfig, ShareProviderSettings } from './share';

const STORAGE_KEY = 'qoredb_share_provider_settings';

export const DEFAULT_SHARE_PROVIDER_SETTINGS: ShareProviderSettings = {
  enabled: false,
  provider_name: '',
  upload_url: '',
  method: 'post',
  body_mode: 'multipart',
  file_field_name: 'file',
  response_url_path: 'url',
};

function asString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

export function getShareProviderSettings(): ShareProviderSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_SHARE_PROVIDER_SETTINGS;

    const parsed = JSON.parse(raw) as Partial<ShareProviderSettings>;
    const method = parsed.method === 'put' ? 'put' : 'post';
    const bodyMode = parsed.body_mode === 'binary' ? 'binary' : 'multipart';

    return {
      enabled: parsed.enabled ?? DEFAULT_SHARE_PROVIDER_SETTINGS.enabled,
      provider_name:
        asString(parsed.provider_name) ?? DEFAULT_SHARE_PROVIDER_SETTINGS.provider_name,
      upload_url: asString(parsed.upload_url) ?? DEFAULT_SHARE_PROVIDER_SETTINGS.upload_url,
      method,
      body_mode: bodyMode,
      file_field_name:
        asString(parsed.file_field_name) ?? DEFAULT_SHARE_PROVIDER_SETTINGS.file_field_name,
      response_url_path:
        asString(parsed.response_url_path) ?? DEFAULT_SHARE_PROVIDER_SETTINGS.response_url_path,
    };
  } catch {
    return DEFAULT_SHARE_PROVIDER_SETTINGS;
  }
}

export function setShareProviderSettings(settings: ShareProviderSettings): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}

export function updateShareProviderSettings(
  patch: Partial<ShareProviderSettings>
): ShareProviderSettings {
  const next = { ...getShareProviderSettings(), ...patch };
  setShareProviderSettings(next);
  return next;
}

export function isShareProviderConfigured(
  settings: ShareProviderSettings = getShareProviderSettings()
): boolean {
  return settings.enabled && settings.upload_url.trim().length > 0;
}

export function toShareProviderConfig(settings: ShareProviderSettings): ShareProviderConfig {
  return {
    provider_name: settings.provider_name?.trim() || undefined,
    upload_url: settings.upload_url.trim(),
    method: settings.method,
    body_mode: settings.body_mode,
    file_field_name:
      settings.body_mode === 'multipart'
        ? settings.file_field_name?.trim() || undefined
        : undefined,
    response_url_path: settings.response_url_path?.trim() || undefined,
  };
}
