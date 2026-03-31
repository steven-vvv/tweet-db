<template>
  <section class="stack">
    <p class="lead">Minimal account view for session state and local username.</p>
    <p v-if="loading" class="hint">Loading account session…</p>
    <p v-else-if="error" class="error">{{ error }}</p>
    <div v-else class="panel">
      <div class="row">
        <span>Status</span>
        <strong>{{ session?.authenticated ? 'Authenticated' : 'Anonymous' }}</strong>
      </div>
      <div class="row">
        <span>Username</span>
        <strong>{{ session?.username ?? '-' }}</strong>
      </div>
      <div class="row">
        <span>Registered</span>
        <strong>{{ session?.registered ? 'Yes' : 'Pending' }}</strong>
      </div>
      <div class="row mono">
        <span>Subject ID</span>
        <strong>{{ session?.subject_id ?? '-' }}</strong>
      </div>
      <div class="row mono">
        <span>Authorization ID</span>
        <strong>{{ session?.authorization_id ?? '-' }}</strong>
      </div>
    </div>
    <button type="button" class="secondary" :disabled="loading || submitting" @click="signOut">
      Sign out
    </button>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import { fetchSession, logout, type SessionMeResponse } from '../api'

const router = useRouter()
const loading = ref(true)
const submitting = ref(false)
const error = ref('')
const session = ref<SessionMeResponse | null>(null)

onMounted(async () => {
  try {
    const current = await fetchSession()
    if (!current.authenticated) {
      await router.replace('/login')
      return
    }
    if (!current.registered) {
      await router.replace('/register')
      return
    }
    session.value = current
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to read session'
  } finally {
    loading.value = false
  }
})

async function signOut() {
  error.value = ''
  submitting.value = true
  try {
    await logout()
    await router.replace('/login')
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to sign out'
  } finally {
    submitting.value = false
  }
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
}

.secondary:disabled {
  opacity: 0.5;
}
</style>
