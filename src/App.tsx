// SPDX-License-Identifier: Apache-2.0

import { AppLayout } from './AppLayout';
import { AuthGate } from './components/Auth/AuthGate';
import { ConfirmHost } from './components/Guard/ConfirmHost';
import { useFrontendReady } from './hooks/useFrontendReady';
import { AiPreferencesProvider } from './providers/AiPreferencesProvider';
import { InterceptorAlertsProvider } from './providers/InterceptorAlertsProvider';
import { LicenseProvider } from './providers/LicenseProvider';
import { ModalProvider } from './providers/ModalProvider';
import { PluginOutputProvider } from './providers/PluginOutputProvider';
import { PluginProvider } from './providers/PluginProvider';
import { SessionProvider } from './providers/SessionProvider';
import { ShortcutProvider } from './providers/ShortcutProvider';
import { TabProvider } from './providers/TabProvider';
import { WorkspaceProvider } from './providers/WorkspaceProvider';

import './index.css';

function App() {
  useFrontendReady();

  return (
    <AuthGate>
      <LicenseProvider>
        <AiPreferencesProvider>
          <TabProvider>
            <ModalProvider>
              <WorkspaceProvider>
                <SessionProvider>
                  <ShortcutProvider>
                    <PluginProvider>
                      <PluginOutputProvider>
                        <InterceptorAlertsProvider>
                          <AppLayout />
                          <ConfirmHost />
                        </InterceptorAlertsProvider>
                      </PluginOutputProvider>
                    </PluginProvider>
                  </ShortcutProvider>
                </SessionProvider>
              </WorkspaceProvider>
            </ModalProvider>
          </TabProvider>
        </AiPreferencesProvider>
      </LicenseProvider>
    </AuthGate>
  );
}

export default App;
