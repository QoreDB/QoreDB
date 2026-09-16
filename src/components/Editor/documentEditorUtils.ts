// SPDX-License-Identifier: Apache-2.0

/** Updates use $set semantics: unchanged fields must not be sent back to storage. */
export function changedDocumentFields(
  initial: Record<string, unknown>,
  edited: Record<string, unknown>
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(edited).filter(
      ([key, value]) => JSON.stringify(value) !== JSON.stringify(initial[key])
    )
  );
}
