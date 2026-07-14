// SPDX-License-Identifier: Apache-2.0

import type { MigrationSummary } from '@/lib/tauri';

export type { MigrationSummary };

/** A migration parsed from its `.sql` file: metadata plus the up/down scripts. */
export interface Migration {
  version: string;
  name: string;
  filename: string;
  up: string;
  down: string;
}
