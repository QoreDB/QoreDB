// SPDX-License-Identifier: Apache-2.0

import { AlertTriangle, RotateCcw } from 'lucide-react';
import { Component, type ErrorInfo, type ReactNode } from 'react';

interface ErrorBoundaryProps {
  children: ReactNode;
  fallbackLabel?: string;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('[ErrorBoundary]', error, info.componentStack);
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      return (
        <div className="flex flex-col items-center justify-center gap-4 h-full w-full p-8 text-center">
          <div className="flex items-center justify-center w-12 h-12 rounded-full bg-destructive/10">
            <AlertTriangle size={24} className="text-destructive" />
          </div>
          <div className="space-y-1">
            <h3 className="text-sm font-medium text-foreground">
              {this.props.fallbackLabel ?? 'Something went wrong'}
            </h3>
            <p className="text-xs text-muted-foreground max-w-sm">
              {this.state.error?.message || 'An unexpected error occurred in this panel.'}
            </p>
          </div>
          <button
            type="button"
            onClick={this.handleReset}
            className="inline-flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-md border border-border hover:bg-muted transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--q-accent)]"
          >
            <RotateCcw size={14} />
            Reload panel
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
