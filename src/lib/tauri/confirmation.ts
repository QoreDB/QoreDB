// SPDX-License-Identifier: Apache-2.0

import { invoke, isWeb } from '@/lib/transport';

const DANGEROUS_OPERATION_ACTION = 'acknowledge_dangerous_query';

/**
 * Single-use backend token for an operation the user has just confirmed on
 * `sessionId`. The desktop backend refuses `acknowledgedDangerous` without it;
 * the web server authorizes through its own session roles instead.
 */
export async function dangerousConfirmationToken(
  sessionId: string,
  acknowledgedDangerous: boolean | undefined
): Promise<string | undefined> {
  if (!acknowledgedDangerous || isWeb) return undefined;
  const { token } = await invoke<{ token: string; expires_in_secs: number }>(
    'request_confirmation_token',
    { action: DANGEROUS_OPERATION_ACTION, sessionId }
  );
  return token;
}
