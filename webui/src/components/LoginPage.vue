<template>
  <section class="stack">
    <p class="lead">
      Sign in through the configured SSO provider, then finish local registration if this is the
      first login.
    </p>
    <p v-if="loading" class="hint">Checking current session…</p>
    <p v-else-if="error" class="error">{{ error }}</p>
    <button type="button" class="primary" :disabled="loading" @click="startLogin">
      Continue to SSO
    </button>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import { fetchLoginUrl, fetchSession } from '../api'

const router = useRouter()
const loading = ref(true)
const error = ref('')

onMounted(async () => {
  try {
    const session = await fetchSession()
    if (session.authenticated && session.registered) {
      await router.replace('/account')
      return
    }
    if (session.authenticated && !session.registered) {
      await router.replace('/register')
      return
    }
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to read session'
  } finally {
    loading.value = false
  }
})

async function startLogin() {
  error.value = ''
  loading.value = true
  try {
    const loginUrl = await fetchLoginUrl()
    window.location.href = loginUrl
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to create login URL'
    loading.value = false
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
</style>
