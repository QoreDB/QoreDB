// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';

interface JSONViewerProps {
  data: unknown;
  initialExpanded?: boolean;
  maxDepth?: number;
}

export function JSONViewer({ data, initialExpanded = true, maxDepth = 5 }: JSONViewerProps) {
  return (
    <div className="font-mono text-sm leading-6 overflow-auto p-4 bg-background h-full text-foreground/90">
      <JSONNode value={data} depth={0} initialExpanded={initialExpanded} maxDepth={maxDepth} />
    </div>
  );
}

interface JSONNodeProps {
  value: unknown;
  keyName?: string;
  depth: number;
  initialExpanded: boolean;
  maxDepth: number;
}

function JSONNode({ value, keyName, depth, initialExpanded, maxDepth }: JSONNodeProps) {
  const [expanded, setExpanded] = useState(initialExpanded && depth < 2);

  const indent = { paddingLeft: `${depth * 1.25}rem` };

  if (value === null) {
    return (
      <div className="flex" style={indent}>
        {keyName && <span className="text-muted-foreground mr-1">"{keyName}": </span>}
        <span className="text-muted-foreground/70 italic">null</span>
      </div>
    );
  }

  if (typeof value === 'boolean') {
    return (
      <div className="flex" style={indent}>
        {keyName && <span className="text-muted-foreground mr-1">"{keyName}": </span>}
        <span className="text-accent">{value ? 'true' : 'false'}</span>
      </div>
    );
  }

  if (typeof value === 'number') {
    return (
      <div className="flex" style={indent}>
        {keyName && <span className="text-muted-foreground mr-1">"{keyName}": </span>}
        <span className="text-blue-500 dark:text-blue-400">{value}</span>
      </div>
    );
  }

  if (typeof value === 'string') {
    return (
      <div className="flex" style={indent}>
        {keyName && <span className="text-muted-foreground mr-1">"{keyName}": </span>}
        <span className="text-success text-opacity-90">"{value}"</span>
      </div>
    );
  }

  if (Array.isArray(value)) {
    if (depth >= maxDepth) {
      return (
        <div className="flex" style={indent}>
          {keyName && <span className="text-muted-foreground mr-1">"{keyName}": </span>}
          <span className="text-muted-foreground">[...{value.length} items]</span>
        </div>
      );
    }

    return (
      <div className="flex flex-col">
        <div
          className="flex items-center cursor-pointer hover:bg-muted/20 rounded px-1 -ml-1 select-none"
          style={indent}
          onClick={() => setExpanded(!expanded)}
        >
          <span className="text-muted-foreground mr-1 w-4 flex justify-center">
            {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          </span>
          {keyName && <span className="text-muted-foreground mr-1">"{keyName}": </span>}
          <span className="text-foreground">[</span>
          {!expanded && <span className="text-muted-foreground ml-1">...{value.length} items</span>}
          {!expanded && <span className="text-foreground ml-1">]</span>}
        </div>
        {expanded && (
          <div className="flex flex-col">
            {value.map((item, i) => (
              <JSONNode
                key={i}
                value={item}
                depth={depth + 1}
                initialExpanded={initialExpanded}
                maxDepth={maxDepth}
              />
            ))}
            <div className="flex" style={indent}>
              <span className="ml-5 text-foreground">]</span>
            </div>
          </div>
        )}
      </div>
    );
  }

  if (typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>);

    if (depth >= maxDepth) {
      return (
        <div className="flex" style={indent}>
          {keyName && <span className="text-muted-foreground mr-1">"{keyName}": </span>}
          <span className="text-muted-foreground">{'{...}'}</span>
        </div>
      );
    }

    return (
      <div className="flex flex-col">
        <div
          className="flex items-center cursor-pointer hover:bg-muted/20 rounded px-1 -ml-1 select-none"
          style={indent}
          onClick={() => setExpanded(!expanded)}
        >
          <span className="text-muted-foreground mr-1 w-4 flex justify-center">
            {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          </span>
          {keyName && <span className="text-muted-foreground mr-1">"{keyName}": </span>}
          <span className="text-foreground">{'{'}</span>
          {!expanded && (
            <span className="text-muted-foreground ml-1">...{entries.length} keys</span>
          )}
          {!expanded && <span className="text-foreground ml-1">{'}'}</span>}
        </div>
        {expanded && (
          <div className="flex flex-col">
            {entries.map(([k, v]) => (
              <JSONNode
                key={k}
                keyName={k}
                value={v}
                depth={depth + 1}
                initialExpanded={initialExpanded}
                maxDepth={maxDepth}
              />
            ))}
            <div className="flex" style={indent}>
              <span className="ml-5 text-foreground">{'}'}</span>
            </div>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="flex" style={indent}>
      {String(value)}
    </div>
  );
}
