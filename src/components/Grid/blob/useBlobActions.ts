// SPDX-License-Identifier: Apache-2.0

import { join, tempDir } from '@tauri-apps/api/path';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { openPath } from '@tauri-apps/plugin-opener';
import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import {
  type BlobKind,
  base64ToUint8Array,
  fileExtensionForKind,
  formatHexDump,
  getDataUri,
  MAX_HEX_DUMP_BYTES,
  sizeBucket,
} from '@/lib/binaryUtils';

interface UseBlobActionsArgs {
  value: string;
  byteSize: number;
  columnName: string;
  dataType: string;
  blobKind: BlobKind | null;
  isTooLarge: boolean;
}

const TEMP_FILE_PREFIX = 'qoredb-blob-';

function sanitizeForFilename(input: string): string {
  return input.replace(/[^a-zA-Z0-9_\-.]/g, '_').slice(0, 64) || 'blob';
}

export function useBlobActions({
  value,
  byteSize,
  columnName,
  dataType,
  blobKind,
  isTooLarge,
}: UseBlobActionsArgs) {
  const { t } = useTranslation();

  const copyBase64 = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(t('blobViewer.copyBase64'));
    } catch {
      toast.error(t('blobViewer.clipboardError'));
    }
  }, [value, t]);

  const copyHex = useCallback(async () => {
    try {
      const bytes = base64ToUint8Array(value, MAX_HEX_DUMP_BYTES);
      const dump = formatHexDump(bytes, MAX_HEX_DUMP_BYTES);
      await navigator.clipboard.writeText(dump);
      toast.success(t('blobViewer.copyHex'));
    } catch {
      toast.error(t('blobViewer.clipboardError'));
    }
  }, [value, t]);

  const copyDataUri = useCallback(async () => {
    if (!blobKind) return;
    try {
      const dataUri = getDataUri(value, blobKind.mime);
      await navigator.clipboard.writeText(dataUri);
      toast.success(t('blobViewer.copyDataUriSuccess'));
    } catch {
      toast.error(t('blobViewer.clipboardError'));
    }
  }, [value, blobKind, t]);

  const download = useCallback(async () => {
    if (isTooLarge) {
      toast.error(t('blobViewer.tooLarge'));
      return;
    }
    try {
      const ext = fileExtensionForKind(blobKind);
      const filePath = await save({
        defaultPath: `${sanitizeForFilename(columnName)}.${ext}`,
        filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
      });

      if (!filePath) return;

      const bytes = base64ToUint8Array(value);
      await writeFile(filePath, bytes);
      toast.success(t('blobViewer.downloadSuccess'));

      AnalyticsService.capture('blob_downloaded', {
        mime: blobKind?.mime ?? 'application/octet-stream',
        size_bucket: sizeBucket(byteSize),
        column_type: dataType,
      });
    } catch (err) {
      console.error('Blob download failed:', err);
      toast.error(t('blobViewer.downloadError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  }, [value, columnName, dataType, blobKind, byteSize, isTooLarge, t]);

  const openExternal = useCallback(async () => {
    if (isTooLarge) {
      toast.error(t('blobViewer.tooLarge'));
      return;
    }
    try {
      const ext = fileExtensionForKind(blobKind);
      const dir = await tempDir();
      const fileName = `${TEMP_FILE_PREFIX}${sanitizeForFilename(columnName)}-${Date.now()}.${ext}`;
      const filePath = await join(dir, fileName);
      const bytes = base64ToUint8Array(value);
      await writeFile(filePath, bytes);
      await openPath(filePath);
    } catch (err) {
      console.error('Open external failed:', err);
      toast.error(t('blobViewer.openExternalError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  }, [value, columnName, blobKind, isTooLarge, t]);

  return { copyBase64, copyHex, copyDataUri, download, openExternal };
}
