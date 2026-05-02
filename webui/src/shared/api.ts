export type JsonRecord = Record<string, any>

export type PublicSessionResponse = {
  authenticated: boolean
  registered: boolean
  username: string | null
  expires_at: string | null
  account_url: string | null
}

export type InternalSessionResponse = {
  authenticated: boolean
  registered: boolean
  is_admin: boolean
  disabled: boolean
  activation_required: boolean
  user_id: string | null
  username: string | null
  subject_id: string | null
  authorization_id: string | null
  disabled_at: string | null
  disabled_by_user_id: string | null
  expires_at: string | null
}

export type CursorListResponse<T extends JsonRecord = JsonRecord> = {
  items: T[]
  nextCursor: string | null
}

export type DetailResponse<
  TSummary extends JsonRecord = JsonRecord,
  TRelated extends JsonRecord = JsonRecord,
> = {
  summary: TSummary
  record: JsonRecord
  related: TRelated
}

export type V2ListResponse<T extends JsonRecord = JsonRecord> = {
  data: T[]
  pagination: {
    limit: number
    nextCursor: string | null
  }
}

export type V2DetailResponse<
  TData extends JsonRecord = JsonRecord,
  TIncluded extends JsonRecord = JsonRecord,
> = {
  data: TData
  included?: TIncluded
}

type QueryValue = string | number | boolean | null | undefined

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers ?? {}),
    },
    ...init,
  })

  if (!response.ok) {
    const text = await response.text()
    throw new Error(text || `${response.status} ${response.statusText}`)
  }

  return response.json() as Promise<T>
}

function withQuery(path: string, params?: Record<string, QueryValue>): string {
  if (!params) {
    return path
  }

  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === '') {
      continue
    }
    search.set(key, String(value))
  }

  const query = search.toString()
  return query ? `${path}?${query}` : path
}

export function fetchPublicSession(): Promise<PublicSessionResponse> {
  return request<PublicSessionResponse>('/api/v1/session', {
    method: 'GET',
  })
}

export function fetchInternalSession(): Promise<InternalSessionResponse> {
  return request<InternalSessionResponse>('/internal/v1/session', {
    method: 'GET',
  })
}

export function fetchV2Me(params?: { include?: string }) {
  return request<V2DetailResponse>(withQuery('/internal/v2/me', params), {
    method: 'GET',
  })
}

export function completeRegistration(username: string) {
  return request<{ user_id: string; username: string }>('/internal/v1/auth/registration', {
    method: 'POST',
    body: JSON.stringify({ username }),
  })
}

export function logout() {
  return request<{ ok: boolean }>('/internal/v1/session', {
    method: 'DELETE',
  })
}

export function fetchAdminUsers(params?: {
  q?: string
  status?: string
  role?: string
  cursor?: string | null
  limit?: number
}) {
  return request<CursorListResponse>(withQuery('/internal/v1/admin/users', params), {
    method: 'GET',
  })
}

export function fetchAdminUser(userId: string) {
  return request<DetailResponse>(`/internal/v1/admin/users/${userId}`, {
    method: 'GET',
  })
}

export function disableAdminUser(userId: string) {
  return request<{ ok: boolean; user: JsonRecord }>(`/internal/v1/admin/users/${userId}/disable`, {
    method: 'POST',
  })
}

export function enableAdminUser(userId: string) {
  return request<{ ok: boolean; user: JsonRecord }>(`/internal/v1/admin/users/${userId}/enable`, {
    method: 'POST',
  })
}

export function fetchAdminOverview() {
  return request<JsonRecord>('/internal/v1/admin/overview', {
    method: 'GET',
  })
}

export function fetchAdminTwitterUsers(params?: {
  q?: string
  sort?: string
  cursor?: string | null
  limit?: number
}) {
  return request<CursorListResponse>(withQuery('/internal/v1/admin/twitter-users', params), {
    method: 'GET',
  })
}

export function fetchAdminTwitterUser(userId: string) {
  return request<DetailResponse>(`/internal/v1/admin/twitter-users/${userId}`, {
    method: 'GET',
  })
}

export function fetchAdminTweets(params?: {
  q?: string
  authorId?: string
  relation?: string
  sort?: string
  cursor?: string | null
  limit?: number
}) {
  return request<CursorListResponse>(withQuery('/internal/v1/admin/tweets', params), {
    method: 'GET',
  })
}

export function fetchV2Tweets(params?: {
  q?: string
  authorId?: string
  relation?: string
  sort?: string
  include?: string
  cursor?: string | null
  limit?: number
}) {
  return request<V2ListResponse>(withQuery('/internal/v2/tweets', params), {
    method: 'GET',
  })
}

export function fetchV2TweetSearch(params?: {
  q?: string
  authorId?: string
  relation?: string
  sort?: string
  include?: string
  cursor?: string | null
  limit?: number
}) {
  return request<V2ListResponse>(withQuery('/internal/v2/search/tweets', params), {
    method: 'GET',
  })
}

export function fetchV2Tweet(tweetId: string, params?: { include?: string }) {
  return request<V2DetailResponse>(
    withQuery(`/internal/v2/tweets/${encodeURIComponent(tweetId)}`, params),
    {
      method: 'GET',
    },
  )
}

export function fetchV2TwitterUser(userId: string, params?: { include?: string }) {
  return request<V2DetailResponse>(
    withQuery(`/internal/v2/twitter-users/${encodeURIComponent(userId)}`, params),
    {
      method: 'GET',
    },
  )
}

export function fetchAdminTweet(tweetId: string) {
  return request<DetailResponse>(`/internal/v1/admin/tweets/${tweetId}`, {
    method: 'GET',
  })
}

export function fetchAdminMediaList(params?: {
  q?: string
  mediaType?: string
  transferStatus?: string
  cursor?: string | null
  limit?: number
}) {
  return request<CursorListResponse>(withQuery('/internal/v1/admin/media', params), {
    method: 'GET',
  })
}

export function fetchAdminMedia(mediaId: string) {
  return request<DetailResponse>(`/internal/v1/admin/media/${mediaId}`, {
    method: 'GET',
  })
}

export function createAdminMediaTransferTask(mediaId: string) {
  return request<{ ok: boolean; created: boolean; taskId: string }>(
    `/internal/v1/admin/media/${mediaId}/transfer-tasks`,
    {
      method: 'POST',
    },
  )
}

export function fetchAdminStorageObjects(params?: {
  q?: string
  cursor?: string | null
  limit?: number
}) {
  return request<CursorListResponse>(withQuery('/internal/v1/admin/storage-objects', params), {
    method: 'GET',
  })
}

export function fetchAdminStorageObject(objectId: string) {
  return request<DetailResponse>(`/internal/v1/admin/storage-objects/${objectId}`, {
    method: 'GET',
  })
}

export function adminStorageObjectOpenUrl(objectId: string) {
  return `/internal/v1/admin/storage-objects/${encodeURIComponent(objectId)}/open`
}

export function fetchTransferOverview() {
  return request<JsonRecord>('/internal/v1/admin/transfers/overview', {
    method: 'GET',
  })
}

export function fetchTransferTasks(params?: {
  q?: string
  status?: string
  cursor?: string | null
  limit?: number
}) {
  return request<CursorListResponse>(withQuery('/internal/v1/admin/transfers/tasks', params), {
    method: 'GET',
  })
}

export function fetchTransferTask(taskId: string) {
  return request<DetailResponse>(`/internal/v1/admin/transfers/tasks/${taskId}`, {
    method: 'GET',
  })
}

export function retryTransferTask(taskId: string) {
  return request<{ ok: boolean; task: JsonRecord }>(
    `/internal/v1/admin/transfers/tasks/${taskId}/retry`,
    {
      method: 'POST',
    },
  )
}

export function cancelTransferTask(taskId: string) {
  return request<{ ok: boolean; task: JsonRecord }>(
    `/internal/v1/admin/transfers/tasks/${taskId}/cancel`,
    {
      method: 'POST',
    },
  )
}

export function releaseTransferTask(taskId: string) {
  return request<{ ok: boolean; task: JsonRecord }>(
    `/internal/v1/admin/transfers/tasks/${taskId}/release`,
    {
      method: 'POST',
    },
  )
}
