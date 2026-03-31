<template>
  <section class="stack">
    <p class="lead">Set a local username after the first SSO login.</p>
    <p v-if="loading" class="hint">Checking registration state…</p>
    <p v-else-if="error" class="error">{{ error }}</p>
    <form class="stack" @submit.prevent="submit">
      <label class="stack">
        <span>Username</span>
        <input v-model="username" type="text" placeholder="username" autocomplete="username" />
      </label>
      <button type="submit" class="primary" :disabled="submitting || loading">
        Complete registration
      </button>
    </form>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import { completeRegistration, fetchSession } from '../api'

const router = useRouter()
const username = ref('')
const loading = ref(true)
const submitting = ref(false)
const error = ref('')

onMounted(async () => {
  try {
    const session = await fetchSession()
    if (!session.authenticated) {
      await router.replace('/login')
      return
    }
    if (session.registered) {
      await router.replace('/account')
      return
    }
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to read session'
  } finally {
    loading.value = false
  }
})

async function submit() {
  error.value = ''
  submitting.value = true
  try {
    await completeRegistration(username.value)
    await router.replace('/account')
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to complete registration'
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
</style>
