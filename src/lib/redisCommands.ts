// SPDX-License-Identifier: Apache-2.0

/**
 * Redis command builder for UI-driven mutations.
 *
 * Produces a textual command string that the backend driver parser
 * (`redis.rs::parse_command`) understands. The parser supports:
 *   - Whitespace-separated tokens
 *   - Double-quoted strings with backslash escape (\\ and \")
 *   - Single-quoted strings taken literally
 *
 * We always wrap user-provided values in double quotes and escape `\` and
 * `"` characters. Keeps the protocol-level concerns out of the UI layer.
 */

export type RedisKeyType = 'string' | 'hash' | 'list' | 'set' | 'zset';

export type ListSide = 'left' | 'right';

/** Quote a value safely for use as a Redis command argument. */
export function quoteRedisArg(value: string): string {
  const escaped = value.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  return `"${escaped}"`;
}

/** Build the argv of a Redis command as a single line. */
function buildCommand(argv: string[]): string {
  return argv.join(' ');
}

export interface SetStringArgs {
  key: string;
  value: string;
  /** TTL in seconds. `0` or `undefined` means no expiration. */
  ttlSeconds?: number;
}

export function buildSetString({ key, value, ttlSeconds }: SetStringArgs): string {
  const argv = ['SET', quoteRedisArg(key), quoteRedisArg(value)];
  if (ttlSeconds && ttlSeconds > 0) {
    argv.push('EX', String(Math.floor(ttlSeconds)));
  }
  return buildCommand(argv);
}

export function buildDeleteKeys(keys: string[]): string {
  if (keys.length === 0) {
    throw new Error('At least one key is required');
  }
  return buildCommand(['DEL', ...keys.map(quoteRedisArg)]);
}

export interface HashFieldArgs {
  key: string;
  field: string;
  value: string;
}

export function buildSetHashField({ key, field, value }: HashFieldArgs): string {
  return buildCommand(['HSET', quoteRedisArg(key), quoteRedisArg(field), quoteRedisArg(value)]);
}

export function buildDeleteHashField({ key, field }: Omit<HashFieldArgs, 'value'>): string {
  return buildCommand(['HDEL', quoteRedisArg(key), quoteRedisArg(field)]);
}

export interface ListPushArgs {
  key: string;
  value: string;
  side: ListSide;
}

export function buildPushListItem({ key, value, side }: ListPushArgs): string {
  const cmd = side === 'left' ? 'LPUSH' : 'RPUSH';
  return buildCommand([cmd, quoteRedisArg(key), quoteRedisArg(value)]);
}

export interface ListPopArgs {
  key: string;
  side: ListSide;
}

export function buildPopListItem({ key, side }: ListPopArgs): string {
  const cmd = side === 'left' ? 'LPOP' : 'RPOP';
  return buildCommand([cmd, quoteRedisArg(key)]);
}

export interface ZSetMemberArgs {
  key: string;
  member: string;
  score: number;
}

export function buildSetZSetMember({ key, member, score }: ZSetMemberArgs): string {
  if (!Number.isFinite(score)) {
    throw new Error('ZSET score must be a finite number');
  }
  return buildCommand(['ZADD', quoteRedisArg(key), String(score), quoteRedisArg(member)]);
}

export function buildRemoveZSetMember({ key, member }: Omit<ZSetMemberArgs, 'score'>): string {
  return buildCommand(['ZREM', quoteRedisArg(key), quoteRedisArg(member)]);
}

export interface SetMemberArgs {
  key: string;
  member: string;
}

export function buildAddSetMember({ key, member }: SetMemberArgs): string {
  return buildCommand(['SADD', quoteRedisArg(key), quoteRedisArg(member)]);
}

export function buildRemoveSetMember({ key, member }: SetMemberArgs): string {
  return buildCommand(['SREM', quoteRedisArg(key), quoteRedisArg(member)]);
}

export interface ExpireArgs {
  key: string;
  ttlSeconds: number;
}

export function buildExpire({ key, ttlSeconds }: ExpireArgs): string {
  if (!Number.isFinite(ttlSeconds) || ttlSeconds <= 0) {
    throw new Error('TTL must be a positive integer');
  }
  return buildCommand(['EXPIRE', quoteRedisArg(key), String(Math.floor(ttlSeconds))]);
}

export interface PersistArgs {
  key: string;
}

export function buildPersist({ key }: PersistArgs): string {
  return buildCommand(['PERSIST', quoteRedisArg(key)]);
}

export interface EvalScriptArgs {
  script: string;
  keys: string[];
  args: string[];
}

export function buildEvalScript({ script, keys, args }: EvalScriptArgs): string {
  if (!script.trim()) {
    throw new Error('Lua script must not be empty');
  }
  const argv = [
    'EVAL',
    quoteRedisArg(script),
    String(keys.length),
    ...keys.map(quoteRedisArg),
    ...args.map(quoteRedisArg),
  ];
  return buildCommand(argv);
}

export interface EvalShaArgs {
  sha: string;
  keys: string[];
  args: string[];
}

export function buildEvalSha({ sha, keys, args }: EvalShaArgs): string {
  if (!/^[a-f0-9]{40}$/i.test(sha)) {
    throw new Error('EVALSHA expects a 40-character hex SHA1');
  }
  const argv = [
    'EVALSHA',
    sha.toLowerCase(),
    String(keys.length),
    ...keys.map(quoteRedisArg),
    ...args.map(quoteRedisArg),
  ];
  return buildCommand(argv);
}

export function buildScriptLoad(script: string): string {
  if (!script.trim()) {
    throw new Error('Lua script must not be empty');
  }
  return buildCommand(['SCRIPT', 'LOAD', quoteRedisArg(script)]);
}

/**
 * Best-effort detection of dangerous Redis calls inside a Lua script body.
 * Returns the matched tokens (uppercase) so the UI can warn the user.
 * Not a substitute for the backend safety classifier — just a guardrail.
 */
export function detectDangerousLuaCalls(script: string): string[] {
  const patterns: { token: string; regex: RegExp }[] = [
    { token: 'FLUSHALL', regex: /redis\s*\.\s*p?call\s*\(\s*['"]FLUSHALL['"]/i },
    { token: 'FLUSHDB', regex: /redis\s*\.\s*p?call\s*\(\s*['"]FLUSHDB['"]/i },
    { token: 'SHUTDOWN', regex: /redis\s*\.\s*p?call\s*\(\s*['"]SHUTDOWN['"]/i },
    { token: 'CONFIG', regex: /redis\s*\.\s*p?call\s*\(\s*['"]CONFIG['"]/i },
    {
      token: 'SCRIPT FLUSH',
      regex: /redis\s*\.\s*p?call\s*\(\s*['"]SCRIPT['"]\s*,\s*['"]FLUSH['"]/i,
    },
    {
      token: 'DEBUG SLEEP',
      regex: /redis\s*\.\s*p?call\s*\(\s*['"]DEBUG['"]\s*,\s*['"]SLEEP['"]/i,
    },
  ];
  const found = new Set<string>();
  for (const { token, regex } of patterns) {
    if (regex.test(script)) {
      found.add(token);
    }
  }
  return [...found];
}
