// SPDX-License-Identifier: Apache-2.0
import { useCallback, useState } from 'react';
import { templateCommand } from '@/lib/tauri';

interface TemplateState {
  data: string | null;
  loading: boolean;
  error: string | null;
}

export function useTemplate() {
  const [state, setState] = useState<TemplateState>({
    data: null,
    loading: false,
    error: null,
  });

  const execute = useCallback(async (input: string) => {
    setState(prev => ({ ...prev, loading: true, error: null }));
    try {
      const result = await templateCommand(input);
      if (result.success && result.data) {
        setState({ data: result.data, loading: false, error: null });
        return result.data;
      }
      throw new Error(result.error || 'Unknown error');
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setState({ data: null, loading: false, error: message });
      throw err;
    }
  }, []);

  return { ...state, execute };
}
