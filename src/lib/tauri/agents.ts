// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@/lib/transport';
import type { VaultResponse } from './types';

export interface McpBinaryStatus {
  path: string | null;
  version: string | null;
}

export async function getMcpBinaryStatus(): Promise<McpBinaryStatus> {
  return invoke('agents_mcp_status');
}

export async function setConnectionExposed(
  projectId: string,
  connectionId: string,
  exposed: boolean
): Promise<VaultResponse> {
  return invoke('set_connection_exposed', { projectId, connectionId, exposed });
}
