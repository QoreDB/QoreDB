// SPDX-License-Identifier: Apache-2.0

import { type ReactNode, useEffect, useState } from 'react';
import {
  type AuthStatus,
  consumeSsoRedirect,
  isAuthenticated,
  isWeb,
  webAuthStatus,
} from '@/lib/transport';
import { AuthScreen } from './AuthScreen';

export function AuthGate({ children }: { children: ReactNode }) {
  if (!isWeb) return <>{children}</>;
  return <WebAuthGate>{children}</WebAuthGate>;
}

function WebAuthGate({ children }: { children: ReactNode }) {
  const [authed, setAuthed] = useState<boolean>(isAuthenticated);
  const [status, setStatus] = useState<AuthStatus | null>(null);
  const [ssoError, setSsoError] = useState<string>();

  useEffect(() => {
    const { error } = consumeSsoRedirect();
    if (error) setSsoError(error);
    if (isAuthenticated()) setAuthed(true);
  }, []);

  useEffect(() => {
    if (authed) return;
    let active = true;
    webAuthStatus()
      .then(s => {
        if (active) setStatus(s);
      })
      .catch(() => {
        if (active) setStatus({ setupRequired: false, ssoEnabled: false });
      });
    return () => {
      active = false;
    };
  }, [authed]);

  if (authed) return <>{children}</>;
  if (!status) return null;
  return <AuthScreen status={status} ssoError={ssoError} onAuthenticated={() => setAuthed(true)} />;
}
