export type SessionMeResponse = {
  authenticated: boolean
  registered: boolean
  username: string | null
  expires_at: string | null
  source_login_url: string
  source_register_url: string
  source_manage_url: string
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

export function fetchSession(): Promise<SessionMeResponse> {
  return request<SessionMeResponse>('/api/v1/session', {
    method: 'GET',
  })
}

export async function fetchLoginUrl(): Promise<string> {
  const response = await request<{ login_url: string }>('/api/v1/auth/login-url', {
    method: 'POST',
    body: JSON.stringify({}),
  })
  return response.login_url
}

export function completeRegistration(username: string) {
  return request<{ user_id: string; username: string }>('/api/v1/auth/registration', {
    method: 'POST',
    body: JSON.stringify({ username }),
  })
}

export function logout() {
  return request<{ ok: boolean }>('/api/v1/session', {
    method: 'DELETE',
  })
}
