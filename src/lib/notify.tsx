// SPDX-License-Identifier: Apache-2.0

import { toast } from 'sonner';

/**
 * Standardized notification types for QoreDB
 * explicitly wrapping sonner to enforce consistent styling and behavior.
 */

interface NotifyOptions {
  description?: string;
  duration?: number;
  action?: {
    label: string;
    onClick: () => void;
  };
}

export const notify = {
  success: (message: string, options?: NotifyOptions) => {
    toast.success(message, {
      description: options?.description,
      duration: options?.duration ?? 2000,
      action: options?.action,
    });
  },

  error: (message: string, error?: unknown, options?: NotifyOptions) => {
    let description = options?.description;

    // If no description provided, try to extract it from the error object
    if (!description && error) {
      if (typeof error === 'string') {
        description = error;
      } else if (error instanceof Error) {
        description = error.message;
      } else if (typeof error === 'object' && error !== null && 'message' in error) {
        // Handle typically structured errors (like from backend)
        description = String((error as { message: unknown }).message);
      } else if (typeof error === 'object' && error !== null && 'error' in error) {
        // Handle { success: false, error: "..." } pattern
        description = String((error as { error: unknown }).error);
      } else {
        description = JSON.stringify(error).slice(0, 100);
      }
    }

    toast.error(message, {
      description,
      duration: options?.duration ?? 5000, // Errors stay longer
      action: options?.action,
    });
  },

  warning: (message: string, options?: NotifyOptions) => {
    toast.warning(message, {
      description: options?.description,
      duration: options?.duration ?? 4000,
      action: options?.action,
    });
  },

  info: (message: string, options?: NotifyOptions) => {
    toast.info(message, {
      description: options?.description,
      duration: options?.duration ?? 3000,
      action: options?.action,
    });
  },

  dismiss: (toastId?: string | number) => {
    toast.dismiss(toastId);
  },
};
