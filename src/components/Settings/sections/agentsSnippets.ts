// SPDX-License-Identifier: Apache-2.0

export interface McpClientSnippet {
  id: 'claudeDesktop' | 'claudeCode' | 'cursor';
  labelKey: string;
  content: string;
}

function shellQuote(value: string): string {
  return /\s/.test(value) ? `"${value}"` : value;
}

/** `workspacePath` is the `.qoredb/` directory of the active workspace; the
 *  server reads that workspace's connections instead of the default vault. */
export function buildMcpSnippets(binaryPath: string, workspacePath?: string): McpClientSnippet[] {
  const args = workspacePath ? ['--workspace', workspacePath] : [];
  const server = args.length > 0 ? { command: binaryPath, args } : { command: binaryPath };
  const json = JSON.stringify({ mcpServers: { qoredb: server } }, null, 2);
  const command = [binaryPath, ...args].map(shellQuote).join(' ');
  return [
    { id: 'claudeDesktop', labelKey: 'settings.agents.config.claudeDesktop', content: json },
    {
      id: 'claudeCode',
      labelKey: 'settings.agents.config.claudeCode',
      content: `claude mcp add qoredb -- ${command}`,
    },
    { id: 'cursor', labelKey: 'settings.agents.config.cursor', content: json },
  ];
}
