import { invoke } from '@tauri-apps/api/core';
import { useCallback, useState } from 'react';

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
      const result = await invoke<{
        success: boolean;
        data?: string;
        error?: string;
      }>('template_command', { input });

      if (result.success && result.data) {
        setState({ data: result.data, loading: false, error: null });
        return result.data;
      } else {
        throw new Error(result.error || 'Unknown error');
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setState({ data: null, loading: false, error: message });
      throw err;
    }
  }, []);

  return { ...state, execute };
}
