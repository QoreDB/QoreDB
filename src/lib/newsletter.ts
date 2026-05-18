// SPDX-License-Identifier: Apache-2.0

const SEEN_KEY = 'qoredb_newsletter_prompt_seen';
const MIN_QUERIES_BEFORE_PROMPT = 20;

export const NEWSLETTER_URL = 'https://www.qoredb.com/newsletter';

export function hasNewsletterPromptBeenSeen(): boolean {
  try {
    return localStorage.getItem(SEEN_KEY) === 'true';
  } catch {
    return false;
  }
}

export function markNewsletterPromptSeen(): void {
  try {
    localStorage.setItem(SEEN_KEY, 'true');
  } catch {
    // ignore
  }
}

export function shouldShowNewsletterPrompt(queryCount: number): boolean {
  if (hasNewsletterPromptBeenSeen()) return false;
  return queryCount >= MIN_QUERIES_BEFORE_PROMPT;
}
