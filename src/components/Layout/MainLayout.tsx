// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';
import { TabBar } from '../Tabs/TabBar';
import './MainLayout.css';

interface MainLayoutProps {
  children: ReactNode;
  sidebar?: ReactNode;
}

/**
 * Main layout component.
 * Note: Currently App.tsx manages the layout directly.
 * This component is kept for potential future use.
 */
export function MainLayout({ children, sidebar }: MainLayoutProps) {
  return (
    <div className="layout">
      {sidebar}
      <main className="layout-main">
        <TabBar />
        <div className="layout-content">{children}</div>
      </main>
    </div>
  );
}
