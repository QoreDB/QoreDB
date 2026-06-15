// SPDX-License-Identifier: Apache-2.0

/**
 * Constants for the Elasticsearch / OpenSearch "Dev Tools" console editor:
 * HTTP methods, common endpoints (autocomplete) and ready-to-run templates.
 */

export const SEARCH_METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'HEAD'] as const;

export interface SearchEndpoint {
  label: string;
  detail?: string;
}

/** Common API endpoints proposed in autocomplete on the method line. */
export const SEARCH_ENDPOINTS: SearchEndpoint[] = [
  { label: '_search', detail: 'Search documents' },
  { label: '_count', detail: 'Count documents' },
  { label: '_doc', detail: 'Document API' },
  { label: '_mapping', detail: 'Field mappings' },
  { label: '_settings', detail: 'Index settings' },
  { label: '_bulk', detail: 'Bulk operations (NDJSON)' },
  { label: '_aliases', detail: 'Manage aliases' },
  { label: '_analyze', detail: 'Test analyzers' },
  { label: '_cat/indices?format=json', detail: 'List indices' },
  { label: '_cat/aliases?format=json', detail: 'List aliases' },
  { label: '_cat/health?format=json', detail: 'Cluster health' },
  { label: '_cluster/health', detail: 'Cluster health' },
  { label: '_cluster/stats', detail: 'Cluster statistics' },
];

export interface SearchTemplate {
  /** i18n key for the human-readable name. */
  nameKey: string;
  query: string;
}

/** Ready-to-run console snippets. */
export const SEARCH_TEMPLATES: SearchTemplate[] = [
  {
    nameKey: 'search.templates.matchAll',
    query: 'GET /my-index/_search\n{\n  "query": { "match_all": {} },\n  "size": 10\n}',
  },
  {
    nameKey: 'search.templates.match',
    query: 'GET /my-index/_search\n{\n  "query": { "match": { "field": "value" } }\n}',
  },
  {
    nameKey: 'search.templates.termsAgg',
    query:
      'GET /my-index/_search\n{\n  "size": 0,\n  "aggs": {\n    "by_field": { "terms": { "field": "field.keyword" } }\n  }\n}',
  },
  {
    nameKey: 'search.templates.createIndex',
    query:
      'PUT /my-index\n{\n  "mappings": {\n    "properties": { "title": { "type": "text" } }\n  }\n}',
  },
  {
    nameKey: 'search.templates.indexDoc',
    query: 'POST /my-index/_doc\n{\n  "title": "hello world"\n}',
  },
  {
    nameKey: 'search.templates.bulk',
    query:
      'POST /_bulk\n{ "index": { "_index": "my-index" } }\n{ "title": "a" }\n{ "index": { "_index": "my-index" } }\n{ "title": "b" }',
  },
  {
    nameKey: 'search.templates.catIndices',
    query: 'GET /_cat/indices?format=json',
  },
];

/** Default editor content for a fresh search query tab. */
export const SEARCH_DEFAULT_QUERY = SEARCH_TEMPLATES[0].query;
