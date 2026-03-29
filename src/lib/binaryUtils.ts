// SPDX-License-Identifier: Apache-2.0

/**
 * Binary data utilities for QoreDB Blob/Binary Viewer.
 *
 * Handles detection of binary column types, base64 conversion,
 * hex dump formatting, image type detection, and file size formatting.
 */

/** Set of database column types that represent binary data, lowercased. */
const BINARY_TYPES = new Set([
  // PostgreSQL
  'bytea',
  // MySQL / MariaDB
  'blob',
  'tinyblob',
  'mediumblob',
  'longblob',
  'binary',
  'varbinary',
  // SQLite
  // 'blob' already listed
  // SQL Server
  // 'binary' already listed
  // 'varbinary' already listed
  'image',
  // DuckDB
  // 'blob' already listed
]);

/**
 * Checks if a database column data_type represents binary data.
 * Handles compound types like "varbinary(255)" by extracting the base type.
 */
export function isBinaryType(dataType: string): boolean {
  const normalized = dataType
    .toLowerCase()
    .replace(/\(.*\)/, '')
    .trim();
  return BINARY_TYPES.has(normalized);
}

/**
 * Decodes a base64 string into a Uint8Array.
 *
 * @param base64 - The base64-encoded string
 * @param maxBytes - Optional limit: only decode up to this many bytes.
 *   When set, truncates the base64 input before decoding to avoid
 *   allocating memory for the full payload (important for hex dump of large blobs).
 */
export function base64ToUint8Array(base64: string, maxBytes?: number): Uint8Array {
  let input = base64;
  if (maxBytes !== undefined && maxBytes > 0) {
    // Calculate base64 chars needed for maxBytes (4 base64 chars = 3 bytes)
    // Round up to a multiple of 4 for valid base64
    const maxBase64Len = Math.ceil((maxBytes * 4) / 3);
    const alignedLen = Math.ceil(maxBase64Len / 4) * 4;
    if (input.length > alignedLen) {
      input = input.slice(0, alignedLen);
    }
  }

  const binaryString = atob(input);
  const length =
    maxBytes !== undefined ? Math.min(binaryString.length, maxBytes) : binaryString.length;
  const bytes = new Uint8Array(length);
  for (let i = 0; i < length; i++) {
    bytes[i] = binaryString.charCodeAt(i);
  }
  return bytes;
}

/**
 * Estimates the byte size from a base64 string length.
 */
export function estimateByteSizeFromBase64(base64: string): number {
  let padding = 0;
  if (base64.endsWith('==')) padding = 2;
  else if (base64.endsWith('=')) padding = 1;
  return Math.floor((base64.length * 3) / 4) - padding;
}

/**
 * Formats a byte count into a human-readable size string.
 */
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const k = 1024;
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), units.length - 1);
  const value = bytes / k ** i;
  return `${value < 10 && i > 0 ? value.toFixed(1) : Math.round(value)} ${units[i]}`;
}

/**
 * Generates a classic hex dump from binary data.
 *
 * Format: "00000000  48 65 6C 6C 6F 20 57 6F  72 6C 64 00 00 00 00 00  |Hello World.....|"
 *
 * @param bytes - The binary data to dump
 * @param maxBytes - Maximum bytes to include (default 10000)
 * @param bytesPerLine - Bytes per line (default 16)
 * @returns Formatted hex dump string
 */
export function formatHexDump(
  bytes: Uint8Array,
  maxBytes: number = 10000,
  bytesPerLine: number = 16
): string {
  const lines: string[] = [];
  const limit = Math.min(bytes.length, maxBytes);

  for (let offset = 0; offset < limit; offset += bytesPerLine) {
    const slice = bytes.slice(offset, Math.min(offset + bytesPerLine, limit));

    // Offset column (8 hex digits)
    const offsetHex = offset.toString(16).padStart(8, '0');

    // Hex bytes column
    const hexParts: string[] = [];
    for (let i = 0; i < bytesPerLine; i++) {
      if (i === bytesPerLine / 2) hexParts.push('');
      if (i < slice.length) {
        hexParts.push(slice[i].toString(16).padStart(2, '0').toUpperCase());
      } else {
        hexParts.push('  ');
      }
    }
    const hexStr = hexParts.join(' ');

    // ASCII column
    const asciiStr = Array.from(slice)
      .map(b => (b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : '.'))
      .join('');

    lines.push(`${offsetHex}  ${hexStr}  |${asciiStr.padEnd(bytesPerLine, ' ')}|`);
  }

  return lines.join('\n');
}

/** Known image magic byte prefixes in base64. */
const IMAGE_SIGNATURES: Array<{ prefix: string; type: ImageType; mime: string }> = [
  { prefix: 'iVBORw0KGgo', type: 'png', mime: 'image/png' },
  { prefix: '/9j/', type: 'jpeg', mime: 'image/jpeg' },
  { prefix: 'R0lGODlh', type: 'gif', mime: 'image/gif' },
  { prefix: 'R0lGODdh', type: 'gif', mime: 'image/gif' },
  { prefix: 'UklGR', type: 'webp', mime: 'image/webp' },
];

export type ImageType = 'png' | 'jpeg' | 'gif' | 'webp';

export interface ImageDetection {
  type: ImageType;
  mime: string;
}

/**
 * Detects the image type from a base64-encoded binary by checking magic bytes.
 * Returns null if the data is not a recognized image format.
 */
export function detectImageType(base64: string): ImageDetection | null {
  for (const sig of IMAGE_SIGNATURES) {
    if (base64.startsWith(sig.prefix)) {
      return { type: sig.type, mime: sig.mime };
    }
  }
  return null;
}

/** Allowed MIME types for data URI preview. Prevents injection of arbitrary MIME types. */
const ALLOWED_PREVIEW_MIMES = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp']);

/**
 * Constructs a data URI from base64 data and a MIME type.
 * Only allows known-safe image MIME types to prevent data URI injection.
 */
export function getDataUri(base64: string, mimeType: string): string {
  if (!ALLOWED_PREVIEW_MIMES.has(mimeType)) {
    throw new Error(`Unsupported MIME type for preview: ${mimeType}`);
  }
  return `data:${mimeType};base64,${base64}`;
}

/** Maximum binary size (in bytes) for which we generate image previews. */
export const MAX_PREVIEW_SIZE = 5 * 1024 * 1024; // 5 MB

/** Maximum bytes to include in hex dump display. */
export const MAX_HEX_DUMP_BYTES = 10000;

/** Maximum binary size (in bytes) for full decode operations (download, base64 display). */
export const MAX_DECODE_SIZE = 50 * 1024 * 1024; // 50 MB
