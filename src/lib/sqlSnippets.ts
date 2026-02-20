// SPDX-License-Identifier: Apache-2.0

export interface SqlSnippet {
  id: string;
  label: string;
  description: string;
  template: string;
}

export const SQL_SNIPPETS: SqlSnippet[] = [
  {
    id: 'select',
    label: 'SELECT',
    description: 'Basic select',
    template: 'SELECT ${columns} FROM ${table};',
  },
  {
    id: 'select_where',
    label: 'SELECT WHERE',
    description: 'Select with where',
    template: 'SELECT ${columns} FROM ${table} WHERE ${condition};',
  },
  {
    id: 'insert',
    label: 'INSERT',
    description: 'Insert row',
    template: 'INSERT INTO ${table} (${columns}) VALUES (${values});',
  },
  {
    id: 'update',
    label: 'UPDATE',
    description: 'Update row',
    template: 'UPDATE ${table} SET ${column} = ${value} WHERE ${condition};',
  },
  {
    id: 'delete',
    label: 'DELETE',
    description: 'Delete row',
    template: 'DELETE FROM ${table} WHERE ${condition};',
  },
  {
    id: 'join',
    label: 'JOIN',
    description: 'Select with join',
    template: 'SELECT ${columns}\nFROM ${table} t\nJOIN ${join_table} j ON t.${key} = j.${key};',
  },
  {
    id: 'create_table',
    label: 'CREATE TABLE',
    description: 'Create table',
    template: 'CREATE TABLE ${table} (\n  ${column} ${type} ${constraints}\n);',
  },
];
