<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>Transfers</h1>
        <p>Media transfer queue and worker state.</p>
      </div>
      <button type="button" :disabled="loadingOverview || loadingTasks" @click="refreshAll">
        Refresh
      </button>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="grid-cards">
      <div v-for="item in stats" :key="item.label" class="stat">
        <span>{{ item.label }}</span>
        <strong>{{ item.value }}</strong>
      </div>
    </section>

    <form class="toolbar" @submit.prevent="reloadTasks">
      <input v-model="q" type="search" placeholder="Task, media, or object key prefix" />
      <select v-model="status">
        <option value="all">All statuses</option>
        <option value="pending">Pending</option>
        <option value="processing">Processing</option>
        <option value="completed">Completed</option>
        <option value="failed">Failed</option>
        <option value="canceled">Canceled</option>
      </select>
      <button type="submit" class="primary" :disabled="loadingTasks">Search</button>
    </form>

    <section class="table">
      <div class="table-head cols">
        <span>Task</span>
        <span>Status</span>
        <span>Attempts</span>
        <span>Media</span>
        <span>Updated</span>
      </div>
      <RouterLink
        v-for="item in tasks"
        :key="item.id"
        :to="`/admin/transfers/tasks/${item.id}`"
        class="table-row cols"
      >
        <div>
          <strong class="mono">{{ item.id }}</strong>
          <div class="muted">{{ item.lastError ?? item.storageObjectKey ?? item.sourceKind }}</div>
        </div>
        <span class="badge" :class="statusTone(item.status)">{{ item.status }}</span>
        <span>{{ countValue(item.attemptCount) }}</span>
        <span class="mono">{{ item.mediaId }}</span>
        <span class="muted">{{ item.updatedAt }}</span>
      </RouterLink>
      <p v-if="!loadingTasks && tasks.length === 0" class="empty">No transfer tasks matched.</p>
    </section>

    <button v-if="nextCursor" type="button" :disabled="loadingTasks" @click="loadMoreTasks">
      Load more
    </button>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'

import { asRecord, countValue, statusTone } from '../../shared/admin-helpers'
import { fetchTransferOverview, fetchTransferTasks, type JsonRecord } from '../../shared/api'

const overview = ref<JsonRecord>({})
const tasks = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const q = ref('')
const status = ref('all')
const loadingOverview = ref(false)
const loadingTasks = ref(false)
const error = ref('')

const statusCounts = computed(() => asRecord(overview.value.statusCounts))
const config = computed(() => asRecord(overview.value.config))
const stats = computed(() => [
  { label: 'Workers', value: countValue(config.value.workerCount) },
  { label: 'Pending', value: countValue(statusCounts.value.pending) },
  { label: 'Processing', value: countValue(statusCounts.value.processing) },
  { label: 'Completed', value: countValue(statusCounts.value.completed) },
  { label: 'Failed', value: countValue(statusCounts.value.failed) },
  { label: 'Canceled', value: countValue(statusCounts.value.canceled) },
])

onMounted(refreshAll)

async function refreshAll() {
  await Promise.all([loadOverview(), reloadTasks()])
}

async function loadOverview() {
  loadingOverview.value = true
  error.value = ''
  try {
    overview.value = await fetchTransferOverview()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load transfer overview'
  } finally {
    loadingOverview.value = false
  }
}

async function reloadTasks() {
  await loadTasks(true)
}

async function loadMoreTasks() {
  await loadTasks(false)
}

async function loadTasks(reset: boolean) {
  loadingTasks.value = true
  error.value = ''
  try {
    const response = await fetchTransferTasks({
      q: q.value.trim() || undefined,
      status: status.value,
      cursor: reset ? undefined : nextCursor.value,
    })
    tasks.value = reset ? response.items : [...tasks.value, ...response.items]
    nextCursor.value = response.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load transfer tasks'
  } finally {
    loadingTasks.value = false
  }
}
</script>

<style scoped>
.cols {
  grid-template-columns: minmax(260px, 1.8fr) 120px 90px minmax(140px, 0.8fr) 220px;
}

@media (max-width: 980px) {
  .cols {
    grid-template-columns: 1fr;
  }
}
</style>
