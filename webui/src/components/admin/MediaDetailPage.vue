<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>{{ summary.id ?? route.params.mediaId }}</h1>
        <p>{{ summary.type ?? 'Media detail' }}</p>
      </div>
      <div class="actions">
        <button type="button" :disabled="submitting" @click="enqueueTransfer">
          Enqueue transfer
        </button>
        <RouterLink
          v-if="summary.storageObjectId"
          class="button-link primary"
          :to="`/admin/storage-objects/${summary.storageObjectId}`"
        >
          Storage object
        </RouterLink>
      </div>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="panel panel-pad kv">
      <div>
        <span>Media ID</span>
        <strong class="mono">{{ summary.id ?? '-' }}</strong>
      </div>
      <div>
        <span>Transfer</span>
        <strong class="badge" :class="statusTone(summary.transferStatus)">
          {{ summary.transferStatus ?? 'none' }}
        </strong>
      </div>
      <div>
        <span>Origin tweet</span>
        <strong class="mono">{{ summary.originTweetId ?? '-' }}</strong>
      </div>
      <div>
        <span>Updated</span>
        <strong>{{ summary.updatedAt ?? '-' }}</strong>
      </div>
    </section>

    <section class="split">
      <section class="panel panel-pad">
        <h2>Tweets</h2>
        <RouterLink
          v-for="item in tweets"
          :key="item.id"
          :to="`/admin/tweets/${item.id}`"
          class="mini-row"
        >
          <strong>{{ item.text || item.id }}</strong>
          <span>{{ item.publishedAt }}</span>
        </RouterLink>
        <p v-if="tweets.length === 0" class="empty">No linked tweets.</p>
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
          <span>{{ item.id }} · {{ item.updatedAt }}</span>
        </RouterLink>
        <p v-if="transferTasks.length === 0" class="empty">No transfer tasks.</p>
      </section>
    </section>

    <JsonPanel title="Latest Resource" :value="related.latestResource" />
    <JsonPanel title="Media Record" :value="detail.record" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

import { asArray, asRecord, statusTone } from '../../admin-helpers'
import {
  createAdminMediaTransferTask,
  fetchAdminMedia,
  type DetailResponse,
} from '../../api'
import JsonPanel from './JsonPanel.vue'

const route = useRoute()
const detail = ref<DetailResponse>({ summary: {}, record: {}, related: {} })
const error = ref('')
const submitting = ref(false)

const summary = computed(() => asRecord(detail.value.summary))
const related = computed(() => asRecord(detail.value.related))
const tweets = computed(() => asArray(related.value.tweets))
const transferTasks = computed(() => asArray(related.value.transferTasks))

onMounted(load)

async function load() {
  error.value = ''
  try {
    detail.value = await fetchAdminMedia(String(route.params.mediaId))
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load media'
  }
}

async function enqueueTransfer() {
  submitting.value = true
  error.value = ''
  try {
    await createAdminMediaTransferTask(String(route.params.mediaId))
    await load()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to enqueue transfer'
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

@media (max-width: 900px) {
  .split {
    grid-template-columns: 1fr;
  }
}
</style>
