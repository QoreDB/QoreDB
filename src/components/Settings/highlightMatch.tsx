// SPDX-License-Identifier: Apache-2.0

/**
 * Highlight matching text in a string for search results
 */
export function highlightMatch(text: string, query: string): React.ReactNode {
  if (!query.trim()) return text;

  const lowerText = text.toLowerCase();
  const lowerQuery = query.toLowerCase();
  const index = lowerText.indexOf(lowerQuery);

  if (index === -1) return text;

  const before = text.slice(0, index);
  const match = text.slice(index, index + query.length);
  const after = text.slice(index + query.length);

  return (
    <>
      {before}
      <mark className="bg-primary/30 text-foreground rounded px-0.5">{match}</mark>
      {after}
    </>
  );
}
