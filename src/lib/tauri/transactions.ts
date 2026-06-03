// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';

// ============================================
// TRANSACTIONS
// ============================================

export async function beginTransaction(sessionId: string): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('begin_transaction', { sessionId });
}

export async function commitTransaction(sessionId: string): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('commit_transaction', { sessionId });
}

export async function rollbackTransaction(sessionId: string): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('rollback_transaction', { sessionId });
}

export async function supportsTransactions(sessionId: string): Promise<boolean> {
  return invoke('supports_transactions', { sessionId });
}
