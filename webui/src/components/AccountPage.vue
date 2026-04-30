<template>
  <section class="stack">
    <p class="lead">
      This page is the only account entrypoint. It handles sign-in, first-time registration, and
      the current session view for tweet-db.
    </p>
    <p v-if="loading" class="hint">Loading account state…</p>
    <p v-if="callbackError" class="error">{{ callbackError }}</p>
    <p v-if="error" class="error">{{ error }}</p>
    <section v-if="!loading && !session?.authenticated" class="stack">
      <p class="hint">
        Sign in through the configured SSO provider. After the callback returns, this page will
        continue with local registration when needed.
      </p>
      <button type="button" class="primary" :disabled="submitting" @click="startLogin">
        Continue to sign in
      </button>
    </section>
    <section v-else-if="!loading && session && !session.registered" class="stack">
      <div class="panel">
        <div class="row">
          <span>Status</span>
          <strong>Authenticated</strong>
        </div>
        <div class="row">
          <span>Local username</span>
          <strong>Pending setup</strong>
        </div>
        <div class="row mono">
          <span>Expires At</span>
          <strong>{{ session.expires_at ?? '-' }}</strong>
        </div>
      </div>
      <form class="stack" @submit.prevent="submitRegistration">
        <label class="stack">
          <span>Username</span>
          <input v-model="username" type="text" placeholder="username" autocomplete="username" />
        </label>
        <button
          type="submit"
          class="primary"
          :disabled="submitting || username.trim().length === 0"
        >
          Complete registration
        </button>
      </form>
    </section>
    <div v-else-if="session" class="panel">
      <div class="row">
        <span>Status</span>
        <strong>{{ session.authenticated ? 'Authenticated' : 'Anonymous' }}</strong>
      </div>
      <div class="row">
        <span>Username</span>
        <strong>{{ session.username ?? '-' }}</strong>
      </div>
      <div class="row">
        <span>Registered</span>
        <strong>{{ session.registered ? 'Yes' : 'Pending' }}</strong>
      </div>
      <div class="row mono">
        <span>Expires At</span>
        <strong>{{ session.expires_at ?? '-' }}</strong>
      </div>
      <div v-if="session.is_admin" class="row">
        <span>Admin console</span>
        <a class="inline-link" href="/admin/overview">Open</a>
      </div>
    </div>
    <a
      v-if="session?.authenticated && session.is_admin"
      href="/admin/overview"
      class="secondary"
    >
      Open admin console
    </a>
    <button
      v-if="session?.authenticated"
      type="button"
      class="secondary"
      :disabled="loading || submitting"
      @click="signOut"
    >
      Sign out
    </button>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'

import {
  completeRegistration,
  fetchInternalSession,
  logout,
  type InternalSessionResponse,
} from '../shared/api'

const loading = ref(true)
const submitting = ref(false)
const error = ref('')
const callbackError = ref('')
const username = ref('')
const session = ref<InternalSessionResponse | null>(null)

onMounted(async () => {
  callbackError.value = readCallbackError()
  await loadSession()
})

async function loadSession() {
  loading.value = true
  error.value = ''
  try {
    session.value = await fetchInternalSession()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to read session'
  } finally {
    loading.value = false
  }
}

function startLogin() {
  window.location.href = '/account/login'
}

async function submitRegistration() {
  error.value = ''
  submitting.value = true
  try {
    await completeRegistration(username.value)
    username.value = ''
    await loadSession()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to complete registration'
  } finally {
    submitting.value = false
  }
}

async function signOut() {
  error.value = ''
  submitting.value = true
  try {
    await logout()
    await loadSession()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to sign out'
  } finally {
    submitting.value = false
  }
}

function readCallbackError() {
  const params = new URLSearchParams(window.location.search)
  const value = params.get('error')
  if (value) {
    window.history.replaceState({}, document.title, window.location.pathname)
  }
  return value ? `SSO callback failed: ${value}` : ''
}
</script>

<style scoped>
.stack {
  display: grid;
  gap: 16px;
}

.lead {
  margin: 0;
  color: #42536e;
}

.hint {
  margin: 0;
  color: #4b6589;
}

.error {
  margin: 0;
  color: #a12231;
}

.panel {
  display: grid;
  gap: 12px;
  padding: 18px;
  border: 1px solid #dbe2ea;
  border-radius: 18px;
  background: #f8fafc;
}

input {
  border: 1px solid #ccd4df;
  border-radius: 12px;
  padding: 12px 14px;
  font: inherit;
}

.primary {
  width: fit-content;
  border: 0;
  border-radius: 999px;
  background: #10203a;
  color: white;
  padding: 12px 20px;
  font: inherit;
}

.primary:disabled {
  opacity: 0.5;
}

.row {
  display: flex;
  justify-content: space-between;
  gap: 24px;
}

.mono strong {
  font-family: "IBM Plex Mono", monospace;
  font-size: 0.825rem;
}

.secondary {
  width: fit-content;
  border: 1px solid #c8d3df;
  border-radius: 999px;
  background: white;
  color: #10203a;
  padding: 12px 20px;
  font: inherit;
  text-decoration: none;
}

.secondary:disabled {
  opacity: 0.5;
}

.inline-link {
  color: #10203a;
  font-weight: 600;
}
</style>
