/**
 * Hook for value parsing and conversion utilities in DataGrid
 * Provides functions to convert between display and storage formats
 */

import { useCallback } from 'react';
import { Value } from '@/lib/tauri';

export interface UseValueParsingReturn {
  getEditableValue: (value: Value) => string;
  parseInputValue: (raw: string, dataType?: string) => Value;
  valuesEqual: (a: Value, b: Value) => boolean;
}

/**
 * Hook providing value parsing utilities for inline editing
 */
export function useValueParsing(): UseValueParsingReturn {
  /**
   * Converts a cell value to its editable string representation
   */
  const getEditableValue = useCallback((value: Value): string => {
    if (value === null) return 'NULL';
    if (typeof value === 'boolean') return value ? 'true' : 'false';
    if (typeof value === 'number') return String(value);
    if (typeof value === 'string') return value;
    if (typeof value === 'object') return JSON.stringify(value);
    return String(value);
  }, []);

  /**
   * Parses user input back to the appropriate Value type based on column data type
   */
  const parseInputValue = useCallback((raw: string, dataType?: string): Value => {
    const trimmed = raw.trim();
    if (trimmed.toLowerCase() === 'null') return null;

    const normalizedType = dataType?.toLowerCase() ?? '';

    // Boolean types
    if (normalizedType.includes('bool')) {
      if (trimmed.toLowerCase() === 'true') return true;
      if (trimmed.toLowerCase() === 'false') return false;
      return raw;
    }

    // Numeric types
    const numericTypes = ['int', 'decimal', 'numeric', 'float', 'double', 'real', 'serial'];
    if (numericTypes.some(type => normalizedType.includes(type))) {
      if (trimmed === '') return '';
      const numericValue = Number(trimmed);
      return Number.isNaN(numericValue) ? raw : numericValue;
    }

    // JSON types
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

  /**
   * Compares two values for equality, handling objects via JSON serialization
   */
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
