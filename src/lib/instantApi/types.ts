// SPDX-License-Identifier: BUSL-1.1

/**
 * Wire types for the Instant Data API (Pro).
 *
 * Mirrors the Rust definitions in `src-tauri/src/api/types.rs`. Keep both
 * sides in sync — fields use the snake_case shape produced by serde.
 */

export type EndpointParamType = 'string' | 'integer' | 'float' | 'bool';

export interface EndpointParam {
  name: string;
  type: EndpointParamType;
  required?: boolean;
  default?: string | null;
}

export type QueryShape = 'rows' | 'object';

export interface EndpointMeta {
  id: string;
  name: string;
  connection_id: string;
  shape: QueryShape;
  params_count: number;
  page_size: number;
  created_at: string;
  updated_at: string;
}

export interface InstantApiStatus {
  running: boolean;
  port: number | null;
  base_url: string | null;
  endpoints_count: number;
  uptime_s: number | null;
  /** True when the running server is serving HTTPS (self-signed cert). */
  tls: boolean;
}

export interface CreateEndpointResponse {
  endpoint: EndpointMeta;
  /** One-shot raw token — capture immediately; cannot be retrieved again. */
  token: string;
}
