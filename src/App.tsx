// SPDX-License-Identifier: Apache-2.0

import { AppLayout } from './AppLayout';
import { AuthGate } from './components/Auth/AuthGate';
import { AiPreferencesProvider } from './providers/AiPreferencesProvider';
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
                        <AppLayout />
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
