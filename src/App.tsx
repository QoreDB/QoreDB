import { TabProvider } from './providers/TabProvider';
import { ModalProvider } from './providers/ModalProvider';
import { SessionProvider } from './providers/SessionProvider';
import { ShortcutProvider } from './providers/ShortcutProvider';
import { AppLayout } from './AppLayout';

import './index.css';

function App() {
  return (
    <TabProvider>
      <ModalProvider>
        <SessionProvider>
          <ShortcutProvider>
            <AppLayout />
          </ShortcutProvider>
        </SessionProvider>
      </ModalProvider>
    </TabProvider>
  );
}

export default App;
