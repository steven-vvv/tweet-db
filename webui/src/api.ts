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
  user_id: string | null
  username: string | null
  subject_id: string | null
  authorization_id: string | null
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

export function fetchAdminPosts(params?: {
  q?: string
  sourceKind?: string
  cursor?: string | null
  limit?: number
}) {
  return request<CursorListResponse>(withQuery('/internal/v1/admin/posts', params), {
    method: 'GET',
  })
}

export function fetchAdminPost(sourceKind: string, sourcePostId: string) {
  return request<DetailResponse>(
    `/internal/v1/admin/posts/${encodeURIComponent(sourceKind)}/${encodeURIComponent(sourcePostId)}`,
    {
      method: 'GET',
    },
  )
}

export function fetchAdminActors(params?: {
  q?: string
  sourceKind?: string
  cursor?: string | null
  limit?: number
}) {
  return request<CursorListResponse>(withQuery('/internal/v1/admin/actors', params), {
    method: 'GET',
  })
}

export function fetchAdminActor(sourceKind: string, sourceActorId: string) {
  return request<DetailResponse>(
    `/internal/v1/admin/actors/${encodeURIComponent(sourceKind)}/${encodeURIComponent(sourceActorId)}`,
    {
      method: 'GET',
    },
  )
}

export function fetchAdminMedia(mediaId: string) {
  return request<DetailResponse>(`/internal/v1/admin/media/${mediaId}`, {
    method: 'GET',
  })
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

export function signAdminStorageObject(objectId: string) {
  return request<{ url: string; expiresAt: string }>(
    `/internal/v1/admin/storage-objects/${objectId}/sign`,
    {
      method: 'POST',
    },
  )
}

export function fetchTransferOverview() {
  return request<JsonRecord>('/internal/v1/admin/transfers/overview', {
    method: 'GET',
  })
}

export function fetchTransferJobs(params?: {
  q?: string
  status?: string
  cursor?: string | null
  limit?: number
}) {
  return request<CursorListResponse>(withQuery('/internal/v1/admin/transfers/jobs', params), {
    method: 'GET',
  })
}

export function fetchTransferJob(jobId: string) {
  return request<DetailResponse>(`/internal/v1/admin/transfers/jobs/${jobId}`, {
    method: 'GET',
  })
}

export function requeueTransferJob(jobId: string) {
  return request<{ ok: boolean; job: JsonRecord }>(
    `/internal/v1/admin/transfers/jobs/${jobId}/requeue`,
    {
      method: 'POST',
    },
  )
}
