// SPDX-License-Identifier: Apache-2.0

import {
  BookOpen,
  Camera,
  Database,
  FileCode,
  GitCompare,
  History,
  ListRestart,
  Network,
  Puzzle,
  Settings,
  Table,
} from 'lucide-react';
import { QoreAiMonoMark } from '@/components/Brand/QoreAiMark';
import type { TabKind } from './tabBarTypes';

export function getTabIcon(type: TabKind) {
  switch (type) {
    case 'query':
      return <FileCode size={14} />;
    case 'table':
      return <Table size={14} />;
    case 'database':
      return <Database size={14} />;
    case 'settings':
      return <Settings size={14} />;
    case 'diff':
      return <GitCompare size={14} />;
    case 'federation':
      return <Network size={14} className="text-accent" />;
    case 'snapshots':
      return <Camera size={14} />;
    case 'notebook':
      return <BookOpen size={14} />;
    case 'time-travel':
      return <History size={14} />;
    case 'migrations':
      return <ListRestart size={14} />;
    case 'plugin-output':
      return <Puzzle size={14} />;
    case 'chat':
      return <QoreAiMonoMark size={14} />;
  }
}

export const isTemporaryTab = (type: TabKind) => type === 'query';
