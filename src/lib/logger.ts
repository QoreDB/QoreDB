// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';

export interface LogEntry {
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
  stack?: string;
  timestamp?: string;
}

const sendLog = async (entry: LogEntry) => {
  try {
    await invoke('log_frontend_message', {
      entry: {
        ...entry,
        timestamp: new Date().toISOString(),
      },
    });
  } catch (err) {
    console.error('Failed to send log to backend:', err);
  }
};

export const logger = {
  info: (message: string) => {
    console.log(message);
    sendLog({ level: 'info', message });
  },
  warn: (message: string) => {
    console.warn(message);
    sendLog({ level: 'warn', message });
  },
  error: (message: string, error?: unknown) => {
    console.error(message, error);
    let stack: string | undefined;

    if (error instanceof Error) {
      stack = error.stack;
    }

    sendLog({ level: 'error', message, stack });
  },
  debug: (message: string) => {
    console.debug(message);
    sendLog({ level: 'debug', message });
  },
};
