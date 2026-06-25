// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';
import type { Value } from '@/lib/tauri';

export interface UseValueParsingReturn {
  getEditableValue: (value: Value) => string;
  parseInputValue: (raw: string, dataType?: string) => Value;
  valuesEqual: (a: Value, b: Value) => boolean;
}

export function useValueParsing(): UseValueParsingReturn {
  const getEditableValue = useCallback((value: Value): string => {
    if (value === null) return 'NULL';
    if (typeof value === 'boolean') return value ? 'true' : 'false';
    if (typeof value === 'number') return String(value);
    if (typeof value === 'string') return value;
    if (typeof value === 'object') return JSON.stringify(value);
    return String(value);
  }, []);

  const parseInputValue = useCallback((raw: string, dataType?: string): Value => {
    const trimmed = raw.trim();
    if (trimmed.toLowerCase() === 'null') return null;

    const normalizedType = dataType?.toLowerCase() ?? '';

    if (normalizedType.includes('bool')) {
      if (trimmed.toLowerCase() === 'true') return true;
      if (trimmed.toLowerCase() === 'false') return false;
      return raw;
    }

    const numericTypes = ['int', 'decimal', 'numeric', 'float', 'double', 'real', 'serial'];
    if (numericTypes.some(type => normalizedType.includes(type))) {
      if (trimmed === '') return '';
      const numericValue = Number(trimmed);
      return Number.isNaN(numericValue) ? raw : numericValue;
    }

    if (normalizedType.includes('json')) {
      if (trimmed === '') return '';
      try {
        return JSON.parse(trimmed);
      } catch {
        return raw;
      }
    }

    return raw;
  }, []);

  const valuesEqual = useCallback((a: Value, b: Value): boolean => {
    if (a === b) return true;
    if (typeof a === 'object' && typeof b === 'object' && a && b) {
      try {
        return JSON.stringify(a) === JSON.stringify(b);
      } catch {
        return false;
      }
    }
    return false;
  }, []);

  return {
    getEditableValue,
    parseInputValue,
    valuesEqual,
  };
}
