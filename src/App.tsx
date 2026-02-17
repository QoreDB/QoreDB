// SPDX-License-Identifier: Apache-2.0

import { TabProvider } from './providers/TabProvider';
import { ModalProvider } from './providers/ModalProvider';
import { SessionProvider } from './providers/SessionProvider';
import { ShortcutProvider } from './providers/ShortcutProvider';
import { LicenseProvider } from './providers/LicenseProvider';
import { AppLayout } from './AppLayout';

import './index.css';

function App() {
  return (
    <LicenseProvider>
      <TabProvider>
        <ModalProvider>
          <SessionProvider>
            <ShortcutProvider>
              <AppLayout />
            </ShortcutProvider>
          </SessionProvider>
        </ModalProvider>
      </TabProvider>
    </LicenseProvider>
  );
}

export default App;
