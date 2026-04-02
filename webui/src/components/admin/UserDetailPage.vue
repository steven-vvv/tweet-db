<template>
  <section class="stack">
    <header class="page-head">
      <div>
        <p class="eyebrow">User Detail</p>
        <h2>{{ summary.username ?? route.params.userId }}</h2>
      </div>
      <div class="actions">
        <button
          v-if="summary.id"
          type="button"
          class="primary"
          :disabled="submitting"
          @click="toggleState"
        >
          {{ summary.disabled ? 'Enable account' : 'Disable account' }}
        </button>
      </div>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="summary-card">
      <div class="summary-row">
        <span>User ID</span>
        <strong>{{ summary.id ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Status</span>
        <strong>{{ summary.disabled ? 'Disabled' : 'Active' }}</strong>
      </div>
      <div class="summary-row">
        <span>Role</span>
        <strong>{{ summary.isAdmin ? 'Admin' : 'User' }}</strong>
      </div>
      <div class="summary-row">
        <span>Created</span>
        <strong>{{ summary.createdAt ?? '-' }}</strong>
      </div>
    </section>

    <section class="mini-grid">
      <section class="panel">
        <h3>Sessions</h3>
        <ul>
          <li v-for="item in sessions" :key="item.selector">{{ item.selector }}</li>
        </ul>
      </section>
      <section class="panel">
        <h3>Authorizations</h3>
        <ul>
          <li v-for="item in authorizations" :key="item.authorization_id">
            {{ item.authorization_id }} · {{ item.status }}
          </li>
        </ul>
      </section>
    </section>

    <JsonPanel title="User Record" :value="detail.record" />
    <JsonPanel title="Related" :value="detail.related" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'

import { asArray, asRecord } from '../../admin-helpers'
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

onMounted(() => {
  void load()
})

async function load() {
  error.value = ''
  try {
    detail.value = await fetchAdminUser(String(route.params.userId))
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load user detail'
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
    error.value = err instanceof Error ? err.message : 'Failed to update user state'
  } finally {
    submitting.value = false
  }
}
</script>

<style scoped>
.stack {
  display: grid;
  gap: 18px;
}

.page-head {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: end;
}

.page-head h2,
.panel h3 {
  margin: 0;
}

.eyebrow {
  margin: 0;
  font-size: 0.74rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: #5e7698;
}

.error {
  margin: 0;
  color: #a12231;
}

.summary-card,
.panel {
  display: grid;
  gap: 12px;
  padding: 18px;
  border: 1px solid #d3ddeb;
  border-radius: 18px;
  background: white;
}

.summary-row {
  display: flex;
  justify-content: space-between;
  gap: 24px;
}

.mini-grid {
  display: grid;
  gap: 16px;
}

.panel ul {
  margin: 0;
  padding-left: 18px;
  display: grid;
  gap: 8px;
  font-family: "IBM Plex Mono", monospace;
  font-size: 0.82rem;
}

.primary {
  width: fit-content;
  border: 0;
  border-radius: 999px;
  padding: 11px 18px;
  background: #10203a;
  color: white;
  font: inherit;
}

@media (min-width: 860px) {
  .mini-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
