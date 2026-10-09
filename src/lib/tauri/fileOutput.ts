// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';

/** Desktop file publication, authorized by the existing filesystem/dialog scope. */
export async function writeTextFileAtomic(path: string, contents: string): Promise<void> {
  await invoke<void>('write_text_file_atomic', { path, contents });
}
