// SPDX-License-Identifier: BUSL-1.1

/**
 * Tauri command bindings for Data Contracts (Pro).
 *
 * Mirrors the Rust commands in `src-tauri/src/commands/contracts.rs`.
 * The streaming `contract.run` event is consumed by `useContractRunEvents`.
 */

import { invoke } from '@/lib/transport';
import { listen, type UnlistenFn } from '@/lib/transport';

import type { Contract, ContractMeta, ContractRun, RuleResult } from './types';

export type ContractRunEvent =
  | {
      type: 'started';
      run_id: string;
      contract_id: string;
      contract_name: string;
      rules_total: number;
    }
  | {
      type: 'rule_started';
      run_id: string;
      contract_id: string;
      rule_id: string;
      rule_type: string;
      index: number;
      total: number;
    }
  | {
      type: 'progress';
      run_id: string;
      contract_id: string;
      result: RuleResult;
      index: number;
      total: number;
    }
  | { type: 'completed'; run_id: string; run: ContractRun }
  | { type: 'failed'; run_id: string; contract_id: string; error: string };

export const CONTRACT_RUN_EVENT = 'contract.run';

export async function listContracts(): Promise<ContractMeta[]> {
  return invoke('list_contracts');
}

export async function loadContract(name: string): Promise<string> {
  return invoke('load_contract', { name });
}

export async function saveContract(name: string, source: string): Promise<void> {
  await invoke('save_contract', { name, source });
}

export async function deleteContract(name: string): Promise<void> {
  await invoke('delete_contract', { name });
}

export async function runContract(
  sessionId: string,
  source: string,
  connectionId?: string
): Promise<ContractRun> {
  return invoke('run_contract', {
    sessionId,
    source,
    connectionId: connectionId ?? null,
  });
}

export async function getContractHistory(name: string, limit?: number): Promise<ContractRun[]> {
  return invoke('get_contract_history', { name, limit: limit ?? null });
}

/**
 * Subscribe to streaming `contract.run` events. Returns the Tauri unlisten
 * function — call it on cleanup to stop receiving events.
 */
export async function onContractRun(
  handler: (event: ContractRunEvent) => void
): Promise<UnlistenFn> {
  return listen<ContractRunEvent>(CONTRACT_RUN_EVENT, e => handler(e.payload));
}

export type { Contract, ContractMeta, ContractRun, RuleResult };
