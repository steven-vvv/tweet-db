import type { JsonRecord } from './api'

export function asRecord(value: unknown): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return {}
  }

  return value as JsonRecord
}

export function asArray(value: unknown): JsonRecord[] {
  return Array.isArray(value) ? (value as JsonRecord[]) : []
}

export function toJson(value: unknown): string {
  return JSON.stringify(value, null, 2)
}

export function textValue(value: unknown, fallback = '-'): string {
  if (value === null || value === undefined || value === '') {
    return fallback
  }
  return String(value)
}

export function booleanLabel(value: unknown): string {
  return value ? 'Yes' : 'No'
}
