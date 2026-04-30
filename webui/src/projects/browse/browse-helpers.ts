import type { JsonRecord } from '../../shared/api'

export type TextEntity = {
  range?: {
    start?: number
    end?: number
  }
  [key: string]: unknown
}

export function asRecord(value: unknown): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return {}
  }
  return value as JsonRecord
}

export function asArray<T = JsonRecord>(value: unknown): T[] {
  return Array.isArray(value) ? (value as T[]) : []
}

export function textValue(value: unknown, fallback = '-'): string {
  if (value === null || value === undefined || value === '') {
    return fallback
  }
  return String(value)
}

export function optionalText(value: unknown): string {
  return value === null || value === undefined ? '' : String(value)
}

export function countValue(value: unknown): string {
  if (typeof value === 'number') {
    return new Intl.NumberFormat().format(value)
  }
  if (typeof value === 'string' && value.trim() !== '' && Number.isFinite(Number(value))) {
    return new Intl.NumberFormat().format(Number(value))
  }
  return '0'
}

export function timeLabel(value: unknown): string {
  const raw = optionalText(value)
  const date = raw ? new Date(raw) : null
  if (!date || Number.isNaN(date.getTime())) {
    return textValue(value)
  }
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(date)
}

export function relationLabel(tweet: JsonRecord): string {
  if (tweet.repostId) {
    return 'Repost'
  }
  if (tweet.quoteTweetId) {
    return 'Quote'
  }
  if (tweet.replyToTweetId) {
    return 'Reply'
  }
  return 'Original'
}

export function authorSnapshot(tweetOrAuthor: JsonRecord): JsonRecord {
  const author = asRecord(tweetOrAuthor.author)
  return asRecord(author.latestSnapshot ?? tweetOrAuthor.latestSnapshot)
}

export function authorStats(tweetOrAuthor: JsonRecord): JsonRecord {
  const author = asRecord(tweetOrAuthor.author)
  return asRecord(author.latestStats ?? tweetOrAuthor.latestStats)
}

export function authorDisplayName(tweetOrAuthor: JsonRecord): string {
  const snapshot = authorSnapshot(tweetOrAuthor)
  return textValue(snapshot.display_name ?? snapshot.displayName ?? snapshot.user_name ?? snapshot.userName)
}

export function authorUserName(tweetOrAuthor: JsonRecord): string {
  const snapshot = authorSnapshot(tweetOrAuthor)
  const handle = optionalText(snapshot.user_name ?? snapshot.userName)
  return handle ? `@${handle}` : textValue(tweetOrAuthor.authorId ?? tweetOrAuthor.id)
}

export function authorAvatar(tweetOrAuthor: JsonRecord): string {
  const snapshot = authorSnapshot(tweetOrAuthor)
  return optionalText(snapshot.avatar_url ?? snapshot.avatarUrl)
}

export function tweetText(tweet: JsonRecord, detail = false): JsonRecord {
  if (detail && tweet.noteText) {
    return asRecord(tweet.noteText)
  }
  return asRecord(tweet.legacyText ?? tweet.noteText)
}

export function latestStats(tweet: JsonRecord): JsonRecord {
  return asRecord(tweet.latestStats)
}

export function mediaItems(tweet: JsonRecord): JsonRecord[] {
  return asArray(tweet.media)
}

export function mediaWarnings(media: JsonRecord): string[] {
  const warnings = asArray<string>(media.sensitivityWarnings)
  if (warnings.length > 0) {
    return warnings.map(String)
  }
  return asArray(media.sensitivityWarningIds).map(String)
}

export function mediaPreviewUrl(media: JsonRecord): string {
  return mediaResourceUrl(media)
}

export function mediaOpenUrl(media: JsonRecord): string {
  return mediaResourceUrl(media)
}

export function mediaResourceUrl(media: JsonRecord): string {
  const resource = asRecord(media.latestResource)
  const mediaUrl = optionalText(resource.mediaUrl ?? resource.media_url)
  if (mediaUrl) {
    return mediaUrl
  }

  const video = asRecord(resource.video)
  const variants = asArray(video.variants)
  const variant = variants.find((item) => optionalText(asRecord(item).url))
  return optionalText(asRecord(variant).url)
}

export function mediaAlt(media: JsonRecord): string {
  return optionalText(media.altText) || `${textValue(media.type, 'media')} ${textValue(media.id)}`
}
