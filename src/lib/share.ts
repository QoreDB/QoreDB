// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';
import type { ExportFormat } from './export';

export type ShareHttpMethod = 'post' | 'put';
export type ShareBodyMode = 'multipart' | 'binary';

export interface ShareProviderConfig {
  provider_name?: string;
  upload_url: string;
  method: ShareHttpMethod;
  body_mode: ShareBodyMode;
  file_field_name?: string;
  response_url_path?: string;
}

export interface ShareProviderSettings extends ShareProviderConfig {
  enabled: boolean;
}

export interface ShareProviderStatus {
  has_token: boolean;
}

export interface SharePrepareResponse {
  share_id: string;
  output_path: string;
  file_name: string;
}

export interface ShareUploadResponse {
  share_url: string;
}

export interface ShareSnapshotRequest {
  snapshot_id: string;
  format: ExportFormat;
  include_headers: boolean;
  table_name?: string;
  limit?: number;
  provider: ShareProviderConfig;
  file_name?: string;
}

export async function sharePrepareExport(
  fileName: string,
  extension: string
): Promise<SharePrepareResponse> {
  return invoke('share_prepare_export', { fileName, extension });
}

export async function shareCleanupExport(shareId: string): Promise<{ success: boolean }> {
  return invoke('share_cleanup_export', { shareId });
}

export async function shareUploadPreparedExport(
  shareId: string,
  provider: ShareProviderConfig
): Promise<ShareUploadResponse> {
  return invoke('share_upload_prepared_export', { shareId, provider });
}

export async function shareSaveProviderToken(token: string): Promise<void> {
  return invoke('share_save_provider_token', { token });
}

export async function shareDeleteProviderToken(): Promise<void> {
  return invoke('share_delete_provider_token');
}

export async function shareGetProviderStatus(): Promise<ShareProviderStatus> {
  return invoke('share_get_provider_status');
}

export async function shareSnapshot(request: ShareSnapshotRequest): Promise<ShareUploadResponse> {
  return invoke('share_snapshot', { request });
}
