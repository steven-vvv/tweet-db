<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>{{ summary.objectKey ?? route.params.objectId }}</h1>
        <p>{{ summary.bucket ?? 'Storage object' }}</p>
      </div>
      <a
        v-if="summary.id"
        class="button-link primary"
        :href="adminStorageObjectOpenUrl(String(summary.id))"
        target="_blank"
        rel="noreferrer"
      >
        Open object
      </a>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="panel panel-pad kv">
      <div>
        <span>Object ID</span>
        <strong class="mono">{{ summary.id ?? '-' }}</strong>
      </div>
      <div>
        <span>Content type</span>
        <strong>{{ summary.contentType ?? '-' }}</strong>
      </div>
      <div>
        <span>Length</span>
        <strong>{{ countValue(summary.contentLength) }}</strong>
      </div>
      <div>
        <span>Created</span>
        <strong>{{ summary.createdAt ?? '-' }}</strong>
      </div>
      <div>
        <span>ETag</span>
        <strong class="mono">{{ summary.etag ?? '-' }}</strong>
      </div>
      <div>
        <span>SHA-256</span>
        <strong class="mono">{{ summary.sha256Hex ?? '-' }}</strong>
      </div>
    </section>

    <section class="panel panel-pad">
      <h2>Transfer tasks</h2>
      <RouterLink
        v-for="item in transferTasks"
        :key="item.id"
        :to="`/admin/transfers/tasks/${item.id}`"
        class="mini-row"
      >
        <strong>{{ item.status }}</strong>
        <span>{{ item.mediaId }} · {{ item.updatedAt }}</span>
      </RouterLink>
      <p v-if="transferTasks.length === 0" class="empty">No transfer tasks.</p>
    </section>

    <JsonPanel title="Storage Record" :value="detail.record" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

import { asArray, asRecord, countValue } from '../../admin-helpers'
import {
  adminStorageObjectOpenUrl,
  fetchAdminStorageObject,
  type DetailResponse,
} from '../../api'
import JsonPanel from './JsonPanel.vue'

const route = useRoute()
const detail = ref<DetailResponse>({ summary: {}, record: {}, related: {} })
const error = ref('')

const summary = computed(() => asRecord(detail.value.summary))
const related = computed(() => asRecord(detail.value.related))
const transferTasks = computed(() => asArray(related.value.transferTasks))

onMounted(load)

async function load() {
  error.value = ''
  try {
    detail.value = await fetchAdminStorageObject(String(route.params.objectId))
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load object'
  }
}
</script>

<style scoped>
h2 {
  margin: 0 0 10px;
  font-size: 1rem;
}

.mini-row {
  display: grid;
  gap: 3px;
  padding: 8px 0;
  border-top: 1px solid #edf1f6;
  color: inherit;
  text-decoration: none;
}

.mini-row:first-of-type {
  border-top: 0;
}

.mini-row span {
  color: #66748a;
  font-size: 0.82rem;
}
</style>
