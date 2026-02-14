const URI_PASSWORD_PATTERN =
  /(\b(?:postgres|postgresql|mysql|mariadb|mongodb|redis|mssql|oracle):\/\/[^:\s/]+:)([^@\s/]+)(@)/gi;
const SECRET_ASSIGNMENT_PATTERN =
  /\b(password|passwd|secret|token|api[_-]?key|access[_-]?key)\b\s*=\s*([^\s,;]+)/gi;
const SQL_SINGLE_QUOTED_PATTERN = /'(?:''|[^'])*'/g;
const JSON_DOUBLE_QUOTED_VALUE_PATTERN = /:\s*"([^"\\]*(?:\\.[^"\\]*)*)"/g;
const JSON_SINGLE_QUOTED_VALUE_PATTERN = /:\s*'([^'\\]*(?:\\.[^'\\]*)*)'/g;

export function redactQuery(query: string): string {
  let redacted = query;

  redacted = redacted.replace(URI_PASSWORD_PATTERN, '$1[REDACTED]$3');
  redacted = redacted.replace(SECRET_ASSIGNMENT_PATTERN, (_, key) => `${key}=[REDACTED]`);
  redacted = redacted.replace(SQL_SINGLE_QUOTED_PATTERN, "'[REDACTED]'");
  redacted = redacted.replace(JSON_DOUBLE_QUOTED_VALUE_PATTERN, ':"[REDACTED]"');
  redacted = redacted.replace(JSON_SINGLE_QUOTED_VALUE_PATTERN, ":'[REDACTED]'");

  return redacted;
}

export function redactText(text: string): string {
  return redactQuery(text);
}
