// SPDX-License-Identifier: Apache-2.0

/**
 * Decoration plugin that highlights MongoDB operators (`"$foo"`) inside
 * the document. The JSON grammar treats them as plain strings, so we
 * overlay a CSS class that the editor theme styles separately.
 *
 * Only keys — i.e. `"$foo"` immediately followed by `:` — are highlighted.
 * Bare `$field` references inside values (like `"$user.name"` in an
 * aggregation expression) keep the default string colour, because
 * treating every `$`-prefixed string as an operator would over-colour
 * legitimate field-path references.
 */

import { type Range, RangeSetBuilder } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  type EditorView,
  type PluginValue,
  ViewPlugin,
  type ViewUpdate,
} from '@codemirror/view';

const OPERATOR_KEY_REGEX = /"\$[A-Za-z][\w]*"(?=\s*:)/g;

const operatorDeco = Decoration.mark({ class: 'cm-mongo-operator' });

function buildDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const ranges: Range<Decoration>[] = [];
  for (const { from, to } of view.visibleRanges) {
    const text = view.state.doc.sliceString(from, to);
    for (const match of text.matchAll(OPERATOR_KEY_REGEX)) {
      const start = from + (match.index ?? 0);
      const end = start + match[0].length;
      ranges.push(operatorDeco.range(start, end));
    }
  }
  // RangeSetBuilder requires ranges in ascending `from` order; `matchAll`
  // already yields them in order within each visible chunk, so iterating
  // chunks sequentially preserves the global order.
  for (const r of ranges) builder.add(r.from, r.to, r.value);
  return builder.finish();
}

class MongoHighlightPlugin implements PluginValue {
  decorations: DecorationSet;

  constructor(view: EditorView) {
    this.decorations = buildDecorations(view);
  }

  update(update: ViewUpdate) {
    if (update.docChanged || update.viewportChanged) {
      this.decorations = buildDecorations(update.view);
    }
  }
}

export const mongoOperatorHighlight = ViewPlugin.fromClass(MongoHighlightPlugin, {
  decorations: v => v.decorations,
});
