<template>
  <section class="stack">
    <header class="page-head">
      <div>
        <p class="eyebrow">Transfer Operations</p>
        <h2>Transfer status</h2>
      </div>
      <p class="hint">Monitor worker configuration, recent failures, and queued jobs.</p>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="stats-grid">
      <section v-for="item in stats" :key="item.label" class="stat-card">
        <span>{{ item.label }}</span>
        <strong>{{ item.value }}</strong>
      </section>
    </section>

    <section class="panel">
      <h3>Configuration</h3>
      <div class="config-grid">
        <div v-for="item in configItems" :key="item.label" class="summary-row">
          <span>{{ item.label }}</span>
          <strong>{{ item.value }}</strong>
        </div>
      </div>
    </section>

    <form class="toolbar" @submit.prevent="reloadJobs">
      <input v-model="q" type="search" placeholder="Search media ID, fetch URL, or object key" />
      <select v-model="status">
        <option value="">All statuses</option>
        <option value="pending">Pending</option>
        <option value="processing">Processing</option>
        <option value="retryable">Retryable</option>
        <option value="succeeded">Succeeded</option>
        <option value="failed">Failed</option>
      </select>
      <button type="submit" class="primary" :disabled="loadingJobs">Search jobs</button>
    </form>

    <section class="panel">
      <h3>Recent failed jobs</h3>
      <RouterLink
        v-for="item in recentFailedJobs"
        :key="item.id"
        :to="`/admin/transfers/jobs/${item.id}`"
        class="link-row"
      >
        <strong>{{ item.id }}</strong>
        <span>{{ item.status }}</span>
        <span>{{ item.updatedAt }}</span>
      </RouterLink>
      <div v-if="recentFailedJobs.length === 0" class="hint">No failed or retryable jobs were found.</div>
    </section>

    <section class="panel">
      <h3>Jobs</h3>
      <RouterLink
        v-for="item in jobs"
        :key="item.id"
        :to="`/admin/transfers/jobs/${item.id}`"
        class="link-row"
      >
        <strong>{{ item.id }}</strong>
        <span>{{ item.status }}</span>
        <span>{{ item.fetchUrl }}</span>
      </RouterLink>
      <div v-if="!loadingJobs && jobs.length === 0" class="hint">No jobs matched the current filter.</div>
    </section>

    <button
      v-if="nextCursor"
      type="button"
      class="secondary"
      :disabled="loadingJobs"
      @click="loadMoreJobs"
    >
      Load more
    </button>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'

import { asArray, asRecord, textValue } from '../../admin-helpers'
import { fetchTransferJobs, fetchTransferOverview, type JsonRecord } from '../../api'

const overview = ref<JsonRecord>({})
const jobs = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const q = ref('')
const status = ref('')
const loadingJobs = ref(false)
const error = ref('')

const statusCounts = computed(() => asRecord(overview.value.statusCounts))
const config = computed(() => asRecord(overview.value.config))
const recentFailedJobs = computed(() => asArray(overview.value.recentFailedJobs))
const stats = computed(() => [
  { label: 'Pending', value: textValue(statusCounts.value.pending, '0') },
  { label: 'Processing', value: textValue(statusCounts.value.processing, '0') },
  { label: 'Retryable', value: textValue(statusCounts.value.retryable, '0') },
  { label: 'Succeeded', value: textValue(statusCounts.value.succeeded, '0') },
  { label: 'Failed', value: textValue(statusCounts.value.failed, '0') },
])
const configItems = computed(() => [
  { label: 'Enabled', value: overview.value.enabled ? 'Yes' : 'No' },
  { label: 'Worker count', value: textValue(config.value.workerCount) },
  { label: 'Chunk size (MB)', value: textValue(config.value.chunkSizeMb) },
  { label: 'Max attempts', value: textValue(config.value.maxAttempts) },
])

onMounted(async () => {
  await loadOverview()
  await reloadJobs()
})

async function loadOverview() {
  error.value = ''
  try {
    overview.value = await fetchTransferOverview()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load transfer overview'
  }
}

async function reloadJobs() {
  await loadJobs(true)
}

async function loadMoreJobs() {
  await loadJobs(false)
}

async function loadJobs(reset: boolean) {
  loadingJobs.value = true
  error.value = ''

  try {
    const response = await fetchTransferJobs({
      q: q.value.trim() || undefined,
      status: status.value || undefined,
      cursor: reset ? undefined : nextCursor.value,
    })
    jobs.value = reset ? response.items : [...jobs.value, ...response.items]
    nextCursor.value = response.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load transfer jobs'
  } finally {
    loadingJobs.value = false
  }
}
</script>

<style scoped>
.stack,
.panel {
  display: grid;
  gap: 18px;
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

.hint,
.error {
  margin: 0;
}

.hint {
  color: #516781;
}

.error {
  color: #a12231;
}

.stats-grid,
.config-grid {
  display: grid;
  gap: 14px;
}

.stat-card,
.panel {
  padding: 18px;
  border: 1px solid #d3ddeb;
  border-radius: 18px;
  background: white;
}

.stat-card strong {
  display: block;
  margin-top: 8px;
  font-size: 1.8rem;
}

.toolbar {
  display: grid;
  gap: 12px;
}

.toolbar input,
.toolbar select {
  width: 100%;
  border: 1px solid #cbd6e2;
  border-radius: 14px;
  padding: 12px 14px;
  font: inherit;
}

.summary-row,
.link-row {
  display: flex;
  justify-content: space-between;
  gap: 18px;
}

.link-row {
  padding: 12px 0;
  color: inherit;
  text-decoration: none;
  border-top: 1px solid #edf2f7;
}

.link-row:first-of-type {
  border-top: 0;
}

.primary,
.secondary {
  width: fit-content;
  border-radius: 999px;
  padding: 11px 18px;
  font: inherit;
}

.primary {
  border: 0;
  background: #10203a;
  color: white;
}

.secondary {
  border: 1px solid #ccd8e6;
  background: white;
  color: #10203a;
}

@media (min-width: 860px) {
  .stats-grid {
    grid-template-columns: repeat(5, minmax(0, 1fr));
  }

  .config-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .toolbar {
    grid-template-columns: minmax(0, 1fr) 180px auto;
  }
}
</style>
