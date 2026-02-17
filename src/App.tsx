// SPDX-License-Identifier: Apache-2.0

import { TabProvider } from './providers/TabProvider';
import { ModalProvider } from './providers/ModalProvider';
import { SessionProvider } from './providers/SessionProvider';
import { ShortcutProvider } from './providers/ShortcutProvider';
import { LicenseProvider } from './providers/LicenseProvider';
import { AiPreferencesProvider } from './providers/AiPreferencesProvider';
import { AppLayout } from './AppLayout';

import './index.css';

function App() {
  return (
    <LicenseProvider>
      <AiPreferencesProvider>
        <TabProvider>
          <ModalProvider>
            <SessionProvider>
              <ShortcutProvider>
                <AppLayout />
              </ShortcutProvider>
            </SessionProvider>
          </ModalProvider>
        </TabProvider>
      </AiPreferencesProvider>
    </LicenseProvider>
  );
}

export default App;
