<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>{{ summary.id ?? route.params.taskId }}</h1>
        <p>{{ summary.mediaId ? `Media ${summary.mediaId}` : 'Transfer task' }}</p>
      </div>
      <div class="actions">
        <button
          v-if="summary.canRetry"
          type="button"
          class="primary"
          :disabled="submitting"
          @click="retryTask"
        >
          Retry
        </button>
        <button
          v-if="summary.canCancel"
          type="button"
          class="danger"
          :disabled="submitting"
          @click="cancelTask"
        >
          Cancel
        </button>
        <button
          v-if="summary.canRelease"
          type="button"
          :disabled="submitting"
          @click="releaseTask"
        >
          Release
        </button>
        <RouterLink
          v-if="summary.mediaId"
          class="button-link"
          :to="`/admin/media/${summary.mediaId}`"
        >
          Media
        </RouterLink>
        <RouterLink
          v-if="summary.storageObjectId"
          class="button-link"
          :to="`/admin/storage-objects/${summary.storageObjectId}`"
        >
          Storage
        </RouterLink>
      </div>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="panel panel-pad kv">
      <div>
        <span>Status</span>
        <strong class="badge" :class="statusTone(summary.status)">{{ summary.status ?? '-' }}</strong>
      </div>
      <div>
        <span>Attempts</span>
        <strong>{{ countValue(summary.attemptCount) }}</strong>
      </div>
      <div>
        <span>Source kind</span>
        <strong>{{ summary.sourceKind ?? '-' }}</strong>
      </div>
      <div>
        <span>Updated</span>
        <strong>{{ summary.updatedAt ?? '-' }}</strong>
      </div>
      <div>
        <span>Claimed by</span>
        <strong>{{ summary.claimedBy ?? '-' }}</strong>
      </div>
      <div>
        <span>Completed</span>
        <strong>{{ summary.completedAt ?? '-' }}</strong>
      </div>
    </section>

    <section v-if="summary.lastError" class="panel panel-pad">
      <h2>Last error</h2>
      <p class="error-text">{{ summary.lastError }}</p>
    </section>

    <section class="panel panel-pad">
      <h2>Source URL</h2>
      <p class="mono source">{{ summary.sourceUrl ?? '-' }}</p>
    </section>

    <JsonPanel title="Task Record" :value="detail.record" />
    <JsonPanel title="Related" :value="detail.related" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

import { asRecord, countValue, statusTone } from '../../shared/admin-helpers'
import {
  cancelTransferTask,
  fetchTransferTask,
  releaseTransferTask,
  retryTransferTask,
  type DetailResponse,
} from '../../shared/api'
import JsonPanel from './JsonPanel.vue'

const route = useRoute()
const detail = ref<DetailResponse>({ summary: {}, record: {}, related: {} })
const error = ref('')
const submitting = ref(false)

const summary = computed(() => asRecord(detail.value.summary))

onMounted(load)

async function load() {
  error.value = ''
  try {
    detail.value = await fetchTransferTask(String(route.params.taskId))
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load transfer task'
  }
}

async function retryTask() {
  await runAction(() => retryTransferTask(String(route.params.taskId)))
}

async function cancelTask() {
  await runAction(() => cancelTransferTask(String(route.params.taskId)))
}

async function releaseTask() {
  await runAction(() => releaseTransferTask(String(route.params.taskId)))
}

async function runAction(action: () => Promise<unknown>) {
  submitting.value = true
  error.value = ''
  try {
    await action()
    await load()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to update transfer task'
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

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.source,
.error-text {
  margin: 0;
  overflow-wrap: anywhere;
}
</style>
