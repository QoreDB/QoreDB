// SPDX-License-Identifier: BUSL-1.1

import { useEffect, useRef } from 'react';
import type { AiChatItem } from '@/hooks/useAiAssistant';
import { AiResponseDisplay } from './AiResponseDisplay';

interface AiMessageThreadProps {
  items: AiChatItem[];
  onInsertQuery?: (query: string) => void;
}

export function AiMessageThread({ items, onInsertQuery }: AiMessageThreadProps) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const lastItem = items[items.length - 1];

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: 'end' });
  }, [lastItem?.content, lastItem?.streaming, items.length]);

  if (items.length === 0) {
    return null;
  }

  return (
    <div className="flex flex-col gap-3">
      {items.map(item =>
        item.role === 'user' ? (
          <div key={item.id} className="self-end max-w-[90%]">
            <div className="rounded-lg bg-accent/10 px-3 py-2 text-sm whitespace-pre-wrap">
              {item.content}
            </div>
          </div>
        ) : (
          <div key={item.id} className="max-w-full">
            <AiResponseDisplay
              response={item.content}
              loading={item.streaming ?? false}
              generatedQuery={item.generatedQuery ?? null}
              safetyAnalysis={item.safetyAnalysis ?? null}
              error={item.error ?? null}
              onInsertQuery={onInsertQuery}
            />
          </div>
        )
      )}
      <div ref={bottomRef} />
    </div>
  );
}
