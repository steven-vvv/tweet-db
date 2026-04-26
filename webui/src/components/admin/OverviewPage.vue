<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>Overview</h1>
        <p>Current database and worker state.</p>
      </div>
      <button type="button" :disabled="loading" @click="load">Refresh</button>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="grid-cards">
      <div v-for="item in stats" :key="item.label" class="stat">
        <span>{{ item.label }}</span>
        <strong>{{ item.value }}</strong>
      </div>
    </section>

    <section class="panel panel-pad">
      <h2>Transfer queue</h2>
      <div class="grid-cards compact">
        <div v-for="item in transferStats" :key="item.label" class="stat">
          <span>{{ item.label }}</span>
          <strong>{{ item.value }}</strong>
        </div>
      </div>
    </section>

    <section class="split">
      <section class="panel panel-pad">
        <h2>Recent tweets</h2>
        <RouterLink
          v-for="item in recentTweets"
          :key="item.id"
          :to="`/admin/tweets/${item.id}`"
          class="mini-row"
        >
          <strong>{{ item.text || item.id }}</strong>
          <span>{{ item.authorUserName ?? item.authorId }} · {{ item.publishedAt }}</span>
        </RouterLink>
        <p v-if="recentTweets.length === 0" class="empty">No tweets found.</p>
      </section>

      <section class="panel panel-pad">
        <h2>Failed transfers</h2>
        <RouterLink
          v-for="item in recentFailedTasks"
          :key="item.id"
          :to="`/admin/transfers/tasks/${item.id}`"
          class="mini-row"
        >
          <strong>{{ item.mediaId }}</strong>
          <span>{{ item.status }} · {{ item.lastError ?? item.updatedAt }}</span>
        </RouterLink>
        <p v-if="recentFailedTasks.length === 0" class="empty">No failed transfers.</p>
      </section>
    </section>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'

import { asArray, asRecord, countValue } from '../../admin-helpers'
import { fetchAdminOverview, type JsonRecord } from '../../api'

const overview = ref<JsonRecord>({})
const loading = ref(false)
const error = ref('')

const accounts = computed(() => asRecord(overview.value.accounts))
const domain = computed(() => asRecord(overview.value.domain))
const transfer = computed(() => asRecord(overview.value.transfer))
const recentTweets = computed(() => asArray(overview.value.recentTweets))
const recentFailedTasks = computed(() => asArray(overview.value.recentFailedTasks))

const stats = computed(() => [
  { label: 'Accounts', value: countValue(accounts.value.total) },
  { label: 'X Users', value: countValue(domain.value.twitterUsers) },
  { label: 'Tweets', value: countValue(domain.value.tweets) },
  { label: 'Media', value: countValue(domain.value.media) },
  { label: 'Storage objects', value: countValue(domain.value.storageObjects) },
])

const transferStats = computed(() => [
  { label: 'Workers', value: countValue(transfer.value.workerCount) },
  { label: 'Pending', value: countValue(transfer.value.pending) },
  { label: 'Processing', value: countValue(transfer.value.processing) },
  { label: 'Completed', value: countValue(transfer.value.completed) },
  { label: 'Failed', value: countValue(transfer.value.failed) },
  { label: 'Canceled', value: countValue(transfer.value.canceled) },
])

onMounted(load)

async function load() {
  loading.value = true
  error.value = ''
  try {
    overview.value = await fetchAdminOverview()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load overview'
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
h2 {
  margin: 0 0 10px;
  font-size: 1rem;
}

.compact {
  grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
}

.split {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.mini-row {
  display: grid;
  gap: 3px;
  padding: 9px 0;
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
