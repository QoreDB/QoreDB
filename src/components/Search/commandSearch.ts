// SPDX-License-Identifier: Apache-2.0

export interface SearchableCommand {
  label: string;
  sublabel?: string;
  keywords?: string[];
}

export function commandMatchesQuery(command: SearchableCommand, query: string): boolean {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (!normalizedQuery) return true;

  return [command.label, command.sublabel, ...(command.keywords ?? [])].some(value =>
    value?.toLocaleLowerCase().includes(normalizedQuery)
  );
}
