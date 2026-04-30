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

export function countValue(value: unknown): string {
  if (typeof value === 'number') {
    return new Intl.NumberFormat().format(value)
  }
  if (typeof value === 'string' && value.trim() !== '' && Number.isFinite(Number(value))) {
    return new Intl.NumberFormat().format(Number(value))
  }
  return textValue(value, '0')
}

export function shortId(value: unknown, head = 8, tail = 6): string {
  const text = textValue(value)
  if (text.length <= head + tail + 3) {
    return text
  }
  return `${text.slice(0, head)}...${text.slice(-tail)}`
}

export function relationLabel(item: JsonRecord): string {
  if (item.repostId) {
    return 'Repost'
  }
  if (item.quoteTweetId) {
    return 'Quote'
  }
  if (item.replyToTweetId) {
    return 'Reply'
  }
  return 'Original'
}

export function statusTone(value: unknown): string {
  switch (String(value ?? '').toLowerCase()) {
    case 'active':
    case 'completed':
    case 'authenticated':
      return 'good'
    case 'pending':
    case 'processing':
      return 'warn'
    case 'disabled':
    case 'failed':
    case 'canceled':
      return 'bad'
    default:
      return 'neutral'
  }
}
