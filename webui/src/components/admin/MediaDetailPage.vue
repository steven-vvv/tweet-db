<template>
  <section class="stack">
    <header class="page-head">
      <div>
        <p class="eyebrow">Media Detail</p>
        <h2>{{ summary.identityValue ?? route.params.mediaId }}</h2>
      </div>
      <div class="action-row">
        <RouterLink
          v-if="summary.storageObjectId"
          :to="`/admin/storage-objects/${summary.storageObjectId}`"
          class="pill-link"
        >
          Open storage object
        </RouterLink>
        <button
          v-if="transferJob?.id && transferJob?.canRequeue"
          type="button"
          class="primary"
          :disabled="submitting"
          @click="requeue"
        >
          Requeue transfer
        </button>
      </div>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="summary-card">
      <div class="summary-row">
        <span>Source kind</span>
        <strong>{{ summary.sourceKind ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Identity kind</span>
        <strong>{{ summary.identityKind ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Transfer status</span>
        <strong>{{ summary.transferStatus ?? 'No transfer job' }}</strong>
      </div>
      <div class="summary-row">
        <span>Last observed</span>
        <strong>{{ summary.lastObservedAt ?? '-' }}</strong>
      </div>
    </section>

    <section class="panel">
      <h3>Source rows</h3>
      <div v-if="sources.length === 0" class="hint">This media is not backed by post media source rows.</div>
      <div v-for="item in sources" :key="item.source_media_id" class="link-row">
        <strong>{{ item.source_media_id }}</strong>
        <span>{{ item.media_type }}</span>
        <span>{{ item.source_post_id }}</span>
      </div>
    </section>

    <JsonPanel title="Managed Media Record" :value="detail.record" />
    <JsonPanel title="Related" :value="detail.related" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

import { asArray, asRecord } from '../../admin-helpers'
import { fetchAdminMedia, requeueTransferJob, type DetailResponse } from '../../api'
import JsonPanel from './JsonPanel.vue'

const route = useRoute()
const detail = ref<DetailResponse>({ summary: {}, record: {}, related: {} })
const error = ref('')
const submitting = ref(false)

const summary = computed(() => asRecord(detail.value.summary))
const related = computed(() => asRecord(detail.value.related))
const sources = computed(() => asArray(related.value.sources))
const transferJob = computed(() => asRecord(related.value.transferJob))

onMounted(() => {
  void load()
})

async function load() {
  error.value = ''

  try {
    detail.value = await fetchAdminMedia(String(route.params.mediaId))
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load media detail'
  }
}

async function requeue() {
  if (!transferJob.value.id) {
    return
  }

  error.value = ''
  submitting.value = true

  try {
    await requeueTransferJob(String(transferJob.value.id))
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

.error,
.hint {
  margin: 0;
}

.error {
  color: #a12231;
}

.hint {
  color: #516781;
}

.summary-card,
.panel {
  padding: 18px;
  border: 1px solid #d3ddeb;
  border-radius: 18px;
  background: white;
}

.summary-row,
.link-row {
  display: flex;
  justify-content: space-between;
  gap: 18px;
}

.link-row {
  padding: 12px 0;
  border-top: 1px solid #edf2f7;
}

.link-row:first-of-type {
  border-top: 0;
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
