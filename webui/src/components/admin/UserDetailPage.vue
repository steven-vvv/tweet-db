<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>{{ summary.username ?? route.params.userId }}</h1>
        <p>Account detail and recent IAM activity.</p>
      </div>
      <button
        v-if="summary.id"
        type="button"
        class="primary"
        :disabled="submitting"
        @click="toggleState"
      >
        {{ summary.disabled ? 'Enable account' : 'Disable account' }}
      </button>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="panel panel-pad kv">
      <div>
        <span>User ID</span>
        <strong class="mono">{{ summary.id ?? '-' }}</strong>
      </div>
      <div>
        <span>Status</span>
        <strong class="badge" :class="statusTone(summary.disabled ? 'disabled' : 'active')">
          {{ summary.disabled ? 'Disabled' : 'Active' }}
        </strong>
      </div>
      <div>
        <span>Role</span>
        <strong>{{ summary.isAdmin ? 'Admin' : 'User' }}</strong>
      </div>
      <div>
        <span>Created</span>
        <strong>{{ summary.createdAt ?? '-' }}</strong>
      </div>
    </section>

    <section class="split">
      <section class="panel panel-pad">
        <h2>Sessions</h2>
        <div v-for="item in sessions" :key="item.selector" class="mini-row">
          <strong class="mono">{{ item.selector }}</strong>
          <span>{{ item.registration_state }} · {{ item.expires_at }}</span>
        </div>
        <p v-if="sessions.length === 0" class="empty">No sessions.</p>
      </section>

      <section class="panel panel-pad">
        <h2>Authorizations</h2>
        <div v-for="item in authorizations" :key="item.authorization_id" class="mini-row">
          <strong class="mono">{{ item.authorization_id }}</strong>
          <span>{{ item.status }} · {{ item.updated_at }}</span>
        </div>
        <p v-if="authorizations.length === 0" class="empty">No authorizations.</p>
      </section>
    </section>

    <JsonPanel title="User Record" :value="detail.record" />
    <JsonPanel title="Related" :value="detail.related" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'

import { asArray, asRecord, statusTone } from '../../admin-helpers'
import { disableAdminUser, enableAdminUser, fetchAdminUser, type DetailResponse } from '../../api'
import JsonPanel from './JsonPanel.vue'

const route = useRoute()
const detail = ref<DetailResponse>({ summary: {}, record: {}, related: {} })
const error = ref('')
const submitting = ref(false)

const summary = computed(() => asRecord(detail.value.summary))
const related = computed(() => asRecord(detail.value.related))
const sessions = computed(() => asArray(related.value.sessions))
const authorizations = computed(() => asArray(related.value.authorizations))

onMounted(load)

async function load() {
  error.value = ''
  try {
    detail.value = await fetchAdminUser(String(route.params.userId))
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load account'
  }
}

async function toggleState() {
  if (!summary.value.id) {
    return
  }
  error.value = ''
  submitting.value = true
  try {
    if (summary.value.disabled) {
      await enableAdminUser(String(summary.value.id))
    } else {
      await disableAdminUser(String(summary.value.id))
    }
    await load()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to update account'
  } finally {
    submitting.value = false
  }
}
</script>

<style scoped>
h2 {
  margin: 0 0 10px;
  font-size: 1rem;
}

.split {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.mini-row {
  display: grid;
  gap: 3px;
  padding: 8px 0;
  border-top: 1px solid #edf1f6;
}

.mini-row:first-of-type {
  border-top: 0;
}

.mini-row span {
  color: #66748a;
  font-size: 0.82rem;
}

@media (max-width: 900px) {
  .split {
    grid-template-columns: 1fr;
  }
}
</style>
