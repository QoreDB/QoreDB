// SPDX-License-Identifier: BUSL-1.1

/**
 * Tauri command bindings for the Instant Data API (Pro).
 *
 * Mirrors the 6 Rust commands in `src-tauri/src/commands/instant_api.rs`.
 * The server runs locally (`127.0.0.1`), and each call requires a valid
 * Pro license — the frontend gates UI entry points via `useLicense`.
 */

import { invoke } from '@tauri-apps/api/core';

import type {
  CreateEndpointResponse,
  EndpointMeta,
  EndpointParam,
  InstantApiStatus,
  QueryShape,
} from './types';

export async function startInstantApi(port?: number): Promise<InstantApiStatus> {
  return invoke('start_instant_api', { port: port ?? null });
}

export async function stopInstantApi(): Promise<InstantApiStatus> {
  return invoke('stop_instant_api');
}

export async function getInstantApiStatus(): Promise<InstantApiStatus> {
  return invoke('get_instant_api_status');
}

export async function listEndpoints(): Promise<EndpointMeta[]> {
  return invoke('list_endpoints');
}

export interface CreateEndpointInput {
  name: string;
  connectionId: string;
  querySource: string;
  params?: EndpointParam[];
  shape?: QueryShape;
  pageSize?: number;
}

export async function createEndpoint(
  input: CreateEndpointInput
): Promise<CreateEndpointResponse> {
  return invoke('create_endpoint', {
    name: input.name,
    connectionId: input.connectionId,
    querySource: input.querySource,
    params: input.params ?? null,
    shape: input.shape ?? null,
    pageSize: input.pageSize ?? null,
  });
}

export async function deleteEndpoint(id: string): Promise<void> {
  await invoke('delete_endpoint', { id });
}

export type {
  CreateEndpointResponse,
  EndpointMeta,
  EndpointParam,
  EndpointParamType,
  InstantApiStatus,
  QueryShape,
} from './types';
