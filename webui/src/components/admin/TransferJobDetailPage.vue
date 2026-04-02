<template>
  <section class="stack">
    <header class="page-head">
      <div>
        <p class="eyebrow">Transfer Job Detail</p>
        <h2>{{ summary.id ?? route.params.jobId }}</h2>
      </div>
      <button
        v-if="summary.canRequeue"
        type="button"
        class="primary"
        :disabled="submitting"
        @click="requeue"
      >
        Requeue
      </button>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="summary-card">
      <div class="summary-row">
        <span>Status</span>
        <strong>{{ summary.status ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Media ID</span>
        <strong>{{ summary.mediaId ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Fetch URL</span>
        <strong>{{ summary.fetchUrl ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Updated</span>
        <strong>{{ summary.updatedAt ?? '-' }}</strong>
      </div>
    </section>

    <section class="panel">
      <h3>Links</h3>
      <div class="action-row">
        <RouterLink
          v-if="managedMedia.id"
          :to="`/admin/media/${managedMedia.id}`"
          class="pill-link"
        >
          Open media
        </RouterLink>
        <RouterLink
          v-if="summary.storageObjectId"
          :to="`/admin/storage-objects/${summary.storageObjectId}`"
          class="pill-link"
        >
          Open storage object
        </RouterLink>
      </div>
    </section>

    <JsonPanel title="Transfer Job Record" :value="detail.record" />
    <JsonPanel title="Related" :value="detail.related" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

import { asRecord } from '../../admin-helpers'
import { fetchTransferJob, requeueTransferJob, type DetailResponse } from '../../api'
import JsonPanel from './JsonPanel.vue'

const route = useRoute()
const detail = ref<DetailResponse>({ summary: {}, record: {}, related: {} })
const error = ref('')
const submitting = ref(false)

const summary = computed(() => asRecord(detail.value.summary))
const related = computed(() => asRecord(detail.value.related))
const managedMedia = computed(() => asRecord(related.value.managedMedia))

onMounted(() => {
  void load()
})

async function load() {
  error.value = ''

  try {
    detail.value = await fetchTransferJob(String(route.params.jobId))
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load transfer job detail'
  }
}

async function requeue() {
  error.value = ''
  submitting.value = true

  try {
    await requeueTransferJob(String(route.params.jobId))
    await load()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to requeue transfer job'
  } finally {
    submitting.value = false
  }
}
</script>

<style scoped>
.stack,
.panel {
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
  padding: 18px;
  border: 1px solid #d3ddeb;
  border-radius: 18px;
  background: white;
}

.summary-row {
  display: flex;
  justify-content: space-between;
  gap: 18px;
}

.action-row {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
}

.pill-link,
.primary {
  width: fit-content;
  border-radius: 999px;
  padding: 11px 18px;
  text-decoration: none;
  font: inherit;
}

.pill-link {
  background: white;
  border: 1px solid #ccd8e6;
  color: #10203a;
}

.primary {
  border: 0;
  background: #10203a;
  color: white;
}
</style>
