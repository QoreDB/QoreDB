// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';
import type { Namespace } from './types';

// ============================================
// ROUTINES (Functions/Procedures)
// ============================================

export type RoutineType = 'Function' | 'Procedure';

export interface Routine {
  namespace: Namespace;
  name: string;
  routine_type: RoutineType;
  arguments: string;
  return_type?: string;
  language?: string;
}

export interface RoutineList {
  routines: Routine[];
  total_count: number;
}

export async function listRoutines(
  sessionId: string,
  namespace: Namespace,
  search?: string,
  page?: number,
  pageSize?: number,
  routineType?: RoutineType
): Promise<{
  success: boolean;
  data?: RoutineList;
  error?: string;
}> {
  return invoke('list_routines', {
    sessionId,
    namespace,
    search,
    page,
    page_size: pageSize,
    routine_type: routineType,
  });
}

// ============================================
// ROUTINE DEFINITION & OPERATIONS
// ============================================

export interface RoutineDefinition {
  name: string;
  namespace: Namespace;
  routine_type: RoutineType;
  definition: string;
  language?: string;
  arguments: string;
  return_type?: string;
}

export interface RoutineOperationResult {
  success: boolean;
  executed_command: string;
  message?: string;
  execution_time_ms: number;
}

export async function getRoutineDefinition(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  routineName: string,
  routineType: RoutineType,
  routineArguments?: string
): Promise<{
  success: boolean;
  definition?: RoutineDefinition;
  error?: string;
}> {
  return invoke('get_routine_definition', {
    sessionId,
    database,
    schema,
    routineName,
    routineType,
    arguments: routineArguments,
  });
}

export async function dropRoutine(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  routineName: string,
  routineType: RoutineType,
  routineArguments?: string,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  result?: RoutineOperationResult;
  error?: string;
}> {
  return invoke('drop_routine', {
    sessionId,
    database,
    schema,
    routineName,
    routineType,
    arguments: routineArguments,
    acknowledgedDangerous,
  });
}

// ============================================
// TRIGGERS
// ============================================

export type TriggerTiming = 'Before' | 'After' | 'InsteadOf';
export type TriggerEvent = 'Insert' | 'Update' | 'Delete' | 'Truncate';

export interface Trigger {
  namespace: Namespace;
  name: string;
  table_name: string;
  timing: TriggerTiming;
  events: TriggerEvent[];
  enabled: boolean;
  function_name?: string;
}

export interface TriggerList {
  triggers: Trigger[];
  total_count: number;
}

export interface TriggerDefinition {
  name: string;
  namespace: Namespace;
  table_name: string;
  timing: TriggerTiming;
  events: TriggerEvent[];
  definition: string;
  enabled: boolean;
  function_name?: string;
}

export interface TriggerOperationResult {
  success: boolean;
  executed_command: string;
  message?: string;
  execution_time_ms: number;
}

export async function listTriggers(
  sessionId: string,
  namespace: Namespace,
  search?: string,
  page?: number,
  pageSize?: number
): Promise<{
  success: boolean;
  data?: TriggerList;
  error?: string;
}> {
  return invoke('list_triggers', {
    sessionId,
    namespace,
    search,
    page,
    page_size: pageSize,
  });
}

export async function getTriggerDefinition(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  triggerName: string
): Promise<{
  success: boolean;
  definition?: TriggerDefinition;
  error?: string;
}> {
  return invoke('get_trigger_definition', {
    sessionId,
    database,
    schema,
    triggerName,
  });
}

export async function dropTrigger(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  triggerName: string,
  tableName: string,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  result?: TriggerOperationResult;
  error?: string;
}> {
  return invoke('drop_trigger', {
    sessionId,
    database,
    schema,
    triggerName,
    tableName,
    acknowledgedDangerous,
  });
}

export async function toggleTrigger(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  triggerName: string,
  tableName: string,
  enable: boolean
): Promise<{
  success: boolean;
  result?: TriggerOperationResult;
  error?: string;
}> {
  return invoke('toggle_trigger', {
    sessionId,
    database,
    schema,
    triggerName,
    tableName,
    enable,
  });
}

// ============================================
// EVENTS (MySQL scheduled tasks)
// ============================================

export type EventStatus = 'Enabled' | 'Disabled' | 'SlavesideDisabled';

export interface DatabaseEvent {
  namespace: Namespace;
  name: string;
  event_type: string;
  interval_value?: string;
  interval_field?: string;
  status: EventStatus;
}

export interface EventList {
  events: DatabaseEvent[];
  total_count: number;
}

export interface EventDefinition {
  name: string;
  namespace: Namespace;
  definition: string;
  status: EventStatus;
}

export interface EventOperationResult {
  success: boolean;
  executed_command: string;
  message?: string;
  execution_time_ms: number;
}

export async function listEvents(
  sessionId: string,
  namespace: Namespace,
  search?: string,
  page?: number,
  pageSize?: number
): Promise<{
  success: boolean;
  data?: EventList;
  error?: string;
}> {
  return invoke('list_events', {
    sessionId,
    namespace,
    search,
    page,
    page_size: pageSize,
  });
}

export async function getEventDefinition(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  eventName: string
): Promise<{
  success: boolean;
  definition?: EventDefinition;
  error?: string;
}> {
  return invoke('get_event_definition', {
    sessionId,
    database,
    schema,
    eventName,
  });
}

export async function dropEvent(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  eventName: string,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  result?: EventOperationResult;
  error?: string;
}> {
  return invoke('drop_event', {
    sessionId,
    database,
    schema,
    eventName,
    acknowledgedDangerous,
  });
}

// ============================================
// SEQUENCES (MariaDB 10.3+)
// ============================================

export interface Sequence {
  namespace: Namespace;
  name: string;
  data_type: string;
  start_value: number;
  min_value: number;
  max_value: number;
  increment: number;
  cycle: boolean;
  cache_size: number;
}

export interface SequenceList {
  sequences: Sequence[];
  total_count: number;
}

export interface SequenceDefinition {
  name: string;
  namespace: Namespace;
  definition: string;
}

export interface SequenceOperationResult {
  success: boolean;
  executed_command: string;
  message?: string;
  execution_time_ms: number;
}

export async function listSequences(
  sessionId: string,
  namespace: Namespace,
  search?: string,
  page?: number,
  pageSize?: number
): Promise<{
  success: boolean;
  data?: SequenceList;
  error?: string;
}> {
  return invoke('list_sequences', {
    sessionId,
    namespace,
    search,
    page,
    page_size: pageSize,
  });
}

export async function getSequenceDefinition(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  sequenceName: string
): Promise<{
  success: boolean;
  definition?: SequenceDefinition;
  error?: string;
}> {
  return invoke('get_sequence_definition', {
    sessionId,
    database,
    schema,
    sequenceName,
  });
}

export async function dropSequence(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  sequenceName: string,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  result?: SequenceOperationResult;
  error?: string;
}> {
  return invoke('drop_sequence', {
    sessionId,
    database,
    schema,
    sequenceName,
    acknowledgedDangerous,
  });
}

export async function cancelQuery(
  sessionId: string,
  queryId?: string
): Promise<{
  success: boolean;
  error?: string;
  query_id?: string;
}> {
  return invoke('cancel_query', { sessionId, queryId });
}
