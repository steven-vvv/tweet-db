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
  user_id: string | null
  username: string | null
  subject_id: string | null
  authorization_id: string | null
  expires_at: string | null
}

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
