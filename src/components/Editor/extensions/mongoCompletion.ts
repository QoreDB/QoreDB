// SPDX-License-Identifier: Apache-2.0

/**
 * MongoDB autocomplete source for CodeMirror 6.
 *
 * Provides completions for:
 * - Collection names after `db.`
 * - Methods after `db.<collection>.`
 * - MongoDB operators after `$`
 * - Field names of the current collection inside object literals
 *
 * The collection at the cursor is inferred by scanning backward for
 * `db.<name>.<method>(` — matching the user-facing mongosh syntax.
 */

import type { Completion, CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import type { useSchemaCache } from '../../../hooks/useSchemaCache';
import type { Namespace } from '../../../lib/tauri';

type SchemaCache = ReturnType<typeof useSchemaCache>;

/** Collection-level query/mutation methods. */
const METHODS: Completion[] = [
  { label: 'find', type: 'method', detail: 'filter, projection' },
  { label: 'findOne', type: 'method', detail: 'filter, projection' },
  { label: 'insertOne', type: 'method', detail: 'document' },
  { label: 'insertMany', type: 'method', detail: '[documents]' },
  { label: 'updateOne', type: 'method', detail: 'filter, update' },
  { label: 'updateMany', type: 'method', detail: 'filter, update' },
  { label: 'replaceOne', type: 'method', detail: 'filter, replacement' },
  { label: 'deleteOne', type: 'method', detail: 'filter' },
  { label: 'deleteMany', type: 'method', detail: 'filter' },
  { label: 'aggregate', type: 'method', detail: '[pipeline]' },
  { label: 'countDocuments', type: 'method', detail: 'filter' },
  { label: 'estimatedDocumentCount', type: 'method' },
  { label: 'distinct', type: 'method', detail: 'field, filter' },
  { label: 'findOneAndUpdate', type: 'method', detail: 'filter, update' },
  { label: 'findOneAndReplace', type: 'method', detail: 'filter, replacement' },
  { label: 'findOneAndDelete', type: 'method', detail: 'filter' },
  { label: 'createIndex', type: 'method', detail: 'keys, options' },
  { label: 'dropIndex', type: 'method', detail: 'name' },
  { label: 'listIndexes', type: 'method' },
];

interface OperatorDef {
  label: string;
  detail?: string;
}

const QUERY_OPERATORS: OperatorDef[] = [
  { label: '$eq', detail: 'matches equal values' },
  { label: '$ne', detail: 'matches not-equal values' },
  { label: '$gt', detail: 'greater than' },
  { label: '$gte', detail: 'greater than or equal' },
  { label: '$lt', detail: 'less than' },
  { label: '$lte', detail: 'less than or equal' },
  { label: '$in', detail: 'matches any value in array' },
  { label: '$nin', detail: 'matches none of the values in array' },
  { label: '$exists', detail: 'matches docs with/without the field' },
  { label: '$type', detail: 'matches by BSON type' },
  { label: '$regex', detail: 'regex match' },
  { label: '$expr', detail: 'aggregation expression in query' },
  { label: '$text', detail: 'text search (requires text index)' },
  { label: '$all', detail: 'array contains all values' },
  { label: '$elemMatch', detail: 'at least one array element matches' },
  { label: '$size', detail: 'array length match' },
  { label: '$mod', detail: 'modulo match' },
  { label: '$where', detail: 'JavaScript predicate (dangerous)' },
  { label: '$jsonSchema', detail: 'JSON schema validation' },
  { label: '$geoWithin' },
  { label: '$geoIntersects' },
  { label: '$near' },
  { label: '$nearSphere' },
];

const LOGICAL_OPERATORS: OperatorDef[] = [
  { label: '$and', detail: 'joins clauses with AND' },
  { label: '$or', detail: 'joins clauses with OR' },
  { label: '$nor', detail: 'joins clauses with NOR' },
  { label: '$not', detail: 'negates a clause' },
];

const UPDATE_OPERATORS: OperatorDef[] = [
  { label: '$set', detail: 'set field value' },
  { label: '$unset', detail: 'remove field' },
  { label: '$inc', detail: 'increment by amount' },
  { label: '$mul', detail: 'multiply by amount' },
  { label: '$min', detail: 'update only if less than' },
  { label: '$max', detail: 'update only if greater than' },
  { label: '$rename', detail: 'rename a field' },
  { label: '$setOnInsert', detail: 'set only on insert' },
  { label: '$currentDate', detail: 'set to current date' },
  { label: '$push', detail: 'append to array' },
  { label: '$pull', detail: 'remove matching from array' },
  { label: '$pullAll', detail: 'remove listed values from array' },
  { label: '$addToSet', detail: 'append if not present' },
  { label: '$pop', detail: 'remove first/last element' },
  { label: '$each', detail: 'push/addToSet modifier' },
  { label: '$position', detail: 'push position modifier' },
  { label: '$slice', detail: 'push slice modifier' },
  { label: '$sort', detail: 'push sort modifier' },
];

const AGGREGATION_STAGES: OperatorDef[] = [
  { label: '$match', detail: 'filters stage' },
  { label: '$project', detail: 'shape output' },
  { label: '$group', detail: 'group by expression' },
  { label: '$sort', detail: 'sort documents' },
  { label: '$limit', detail: 'limit output count' },
  { label: '$skip', detail: 'skip documents' },
  { label: '$unwind', detail: 'deconstruct array field' },
  { label: '$lookup', detail: 'left outer join' },
  { label: '$count', detail: 'count documents' },
  { label: '$addFields', detail: 'add/overwrite fields' },
  { label: '$set', detail: 'alias of $addFields' },
  { label: '$unset', detail: 'remove fields' },
  { label: '$replaceRoot', detail: 'replace document root' },
  { label: '$replaceWith', detail: 'replace document root' },
  { label: '$facet', detail: 'parallel sub-pipelines' },
  { label: '$bucket', detail: 'categorize into buckets' },
  { label: '$bucketAuto', detail: 'automatic buckets' },
  { label: '$sample', detail: 'random sample' },
  { label: '$sortByCount', detail: 'group + sort by count' },
  { label: '$graphLookup', detail: 'recursive lookup' },
];

const AGGREGATION_EXPRESSIONS: OperatorDef[] = [
  { label: '$sum' },
  { label: '$avg' },
  { label: '$min' },
  { label: '$max' },
  { label: '$first' },
  { label: '$last' },
  { label: '$push' },
  { label: '$addToSet' },
  { label: '$concat' },
  { label: '$substr' },
  { label: '$toLower' },
  { label: '$toUpper' },
  { label: '$dateToString' },
  { label: '$dateFromString' },
  { label: '$cond' },
  { label: '$ifNull' },
  { label: '$switch' },
  { label: '$literal' },
  { label: '$arrayElemAt' },
  { label: '$map' },
  { label: '$reduce' },
  { label: '$filter' },
  { label: '$size' },
];

function toCompletions(defs: OperatorDef[], type: string): Completion[] {
  return defs.map(def => ({
    label: def.label,
    type,
    detail: def.detail,
  }));
}

const ALL_OPERATORS: Completion[] = [
  ...toCompletions(QUERY_OPERATORS, 'keyword'),
  ...toCompletions(LOGICAL_OPERATORS, 'keyword'),
  ...toCompletions(UPDATE_OPERATORS, 'keyword'),
  ...toCompletions(AGGREGATION_STAGES, 'keyword'),
  ...toCompletions(AGGREGATION_EXPRESSIONS, 'function'),
];

/**
 * Scans backward from `pos` for the nearest `db.<collection>.` pattern.
 * Returns the collection name or null if not found within `windowChars`.
 */
function findCollectionAtCursor(text: string, pos: number, windowChars = 2048): string | null {
  const start = Math.max(0, pos - windowChars);
  const slice = text.slice(start, pos);
  // Match the LAST occurrence of `db.<name>.`
  const re = /\bdb\s*\.\s*([A-Za-z_][\w-]*)\s*\./g;
  let last: string | null = null;
  for (const match of slice.matchAll(re)) {
    last = match[1];
  }
  return last;
}

export interface MongoCompletionContext {
  getSessionId: () => string | null | undefined;
  getNamespace: () => Namespace | null | undefined;
  getSchemaCache: () => SchemaCache;
}

/**
 * Builds the CodeMirror completion source for MongoDB editors.
 *
 * Strategy:
 *  - `db.` alone → propose collection names
 *  - `db.<name>.` → propose collection methods
 *  - token begins with `$` → propose MongoDB operators
 *  - otherwise → propose field names from the resolved collection
 */
export function createMongoCompletionSource(ctx: MongoCompletionContext) {
  // In-memory per-editor cache of collection fields so we don't re-fetch
  // on every keystroke. The schema cache already TTLs but this avoids an
  // async hop inside the completion path.
  const fieldsByCollection = new Map<string, string[]>();

  async function loadFields(collection: string): Promise<string[]> {
    const cached = fieldsByCollection.get(collection);
    if (cached) return cached;
    const ns = ctx.getNamespace();
    if (!ns) return [];
    const schema = await ctx.getSchemaCache().getTableSchema(ns, collection);
    const fields = schema?.columns?.map(c => c.name) ?? [];
    fieldsByCollection.set(collection, fields);
    return fields;
  }

  return async function completionSource(
    context: CompletionContext
  ): Promise<CompletionResult | null> {
    const sessionId = ctx.getSessionId();
    if (!sessionId) return null;

    const doc = context.state.doc.toString();
    const pos = context.pos;

    // 1) Token starting with `$` → operator suggestions
    const dollarTok = context.matchBefore(/\$[A-Za-z]*/);
    if (dollarTok && (dollarTok.from !== dollarTok.to || context.explicit)) {
      return {
        from: dollarTok.from,
        to: dollarTok.to,
        options: ALL_OPERATORS,
        validFor: /^\$[A-Za-z]*$/,
      };
    }

    // 2) Word boundary check
    const word = context.matchBefore(/[\w$]+/);
    if (!word || (word.from === word.to && !context.explicit)) return null;

    // 3) Look at the character just before the word to detect `db.` or `db.X.`
    const before = doc.slice(Math.max(0, word.from - 32), word.from);

    // Match `db.` just before → propose collection names
    if (/\bdb\s*\.\s*$/.test(before)) {
      try {
        const ns = ctx.getNamespace();
        if (!ns) return null;
        const collections = await ctx.getSchemaCache().getCollections(ns);
        return {
          from: word.from,
          to: word.to,
          options: collections.map(c => ({
            label: c.name,
            type: c.collection_type === 'View' ? 'view' : 'class',
          })),
          validFor: /^[\w-]*$/,
        };
      } catch {
        return null;
      }
    }

    // Match `db.<name>.` just before → propose methods
    if (/\bdb\s*\.\s*[A-Za-z_][\w-]*\s*\.\s*$/.test(before)) {
      return {
        from: word.from,
        to: word.to,
        options: METHODS,
        validFor: /^\w*$/,
      };
    }

    // 4) Otherwise, propose field names from the active collection
    const collection = findCollectionAtCursor(doc, pos);
    if (collection) {
      try {
        const fields = await loadFields(collection);
        if (fields.length > 0) {
          return {
            from: word.from,
            to: word.to,
            options: fields.map(f => ({ label: f, type: 'property' })),
            validFor: /^[\w.]*$/,
          };
        }
      } catch {
        // fall through to null
      }
    }

    return null;
  };
}
