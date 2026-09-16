// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, posix, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const entryDocs = new Set([
  'README.md',
  'CONTRIBUTING.md',
  'doc/README.md',
  'doc/audits/README.md',
  'doc/audits/AGENT_READINESS.md',
]);
const instructionName = /(?:^|\/)(?:AGENTS|CLAUDE)\.md$/;

export function localLinks(markdown, source) {
  const prose = markdown
    .replace(/^\s*(`{3,}|~{3,})[^\n]*\n[\s\S]*?^\s*\1\s*$/gm, '')
    .replace(/`[^`\n]*`/g, '');
  // Contributor docs use inline links and reference definitions, not embedded HTML.
  const destinations = [
    ...prose.matchAll(/\[[^\]\n]*\]\(\s*(<[^>]+>|[^\s)]+)(?:\s+"[^"]*")?\s*\)/g),
    ...prose.matchAll(/^\s*\[[^\]]+\]:\s*(<[^>]+>|\S+)/gm),
  ];
  return destinations.flatMap(match => {
    const target = match[1].replace(/^<|>$/g, '');
    if (/^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i.test(target)) return [];
    const path = decodeURIComponent(target.split(/[?#]/)[0]);
    if (!path) return [];
    return [posix.normalize(path.startsWith('/') ? path.slice(1) : posix.join(posix.dirname(source), path))];
  });
}

export function checkRepository(root, candidates) {
  const files = [...new Set(candidates)].filter(file =>
    !file.startsWith('src-tauri/vendor/') &&
    !file.startsWith('doc/private/') &&
    existsSync(resolve(root, file))
  );
  const errors = [];
  const read = file => readFileSync(resolve(root, file), 'utf8');
  const available = target => files.some(file => file === target || file.startsWith(`${target.replace(/\/$/, '')}/`));
  const docs = files.filter(file =>
    entryDocs.has(file) || instructionName.test(file) ||
    (file.startsWith('doc/development/') && file.endsWith('.md'))
  );

  for (const file of docs) {
    for (const target of localLinks(read(file), file)) {
      if (!available(target)) errors.push(`${file}: unavailable local link: ${target}`);
    }
  }

  for (const file of files.filter(file => instructionName.test(file))) {
    const content = read(file);
    if (posix.basename(file) === 'CLAUDE.md') {
      if (content.trim() !== '@AGENTS.md') errors.push(`${file}: must only import @AGENTS.md`);
      const sibling = posix.join(posix.dirname(file), 'AGENTS.md');
      if (!files.includes(sibling)) errors.push(`${file}: missing ${sibling}`);
    } else {
      const budget = file === 'AGENTS.md' ? 8192 : 4096;
      if (Buffer.byteLength(content) > budget) errors.push(`${file}: exceeds ${budget}-byte instruction budget`);
      const sibling = posix.join(posix.dirname(file), 'CLAUDE.md');
      if (!files.includes(sibling)) errors.push(`${file}: missing Claude import ${sibling}`);
    }
  }
  for (const required of ['AGENTS.md', 'CLAUDE.md', 'doc/README.md', 'doc/audits/README.md']) {
    if (!files.includes(required)) errors.push(`Missing entry point: ${required}`);
  }

  const indexed = new Set(['doc/README.md', 'doc/audits/README.md'].flatMap(file =>
    files.includes(file) ? localLinks(read(file), file) : []
  ));
  for (const file of files) {
    if (file.startsWith('doc/') && /\.(?:md|csv)$/.test(file) &&
        !file.startsWith('doc/archive/') && !instructionName.test(file) &&
        file !== 'doc/README.md' && !indexed.has(file)) {
      errors.push(`${file}: add a link from doc/README.md or doc/audits/README.md`);
    }
    if (/\.(?:ts|tsx|rs)$/.test(file) &&
        !/^\/\/ SPDX-License-Identifier: (?:Apache-2\.0|BUSL-1\.1)\r?\n/.test(read(file))) {
      errors.push(`${file}: missing or invalid first-line SPDX header`);
    }
  }
  return { errors, documents: docs.length, files: files.length };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const files = execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], {
    cwd: root,
    encoding: 'utf8',
  }).split('\0').filter(Boolean);
  const result = checkRepository(root, files);
  if (result.errors.length) {
    console.error(result.errors.join('\n'));
    process.exitCode = 1;
  } else {
    console.log(`Repository checks passed (${result.documents} maintained documents, ${result.files} first-party files).`);
  }
}
