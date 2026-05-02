<template>
  <MobileShell title="Home">
    <template #action>
      <button class="text-button" type="button" :disabled="loading" @click="reload">Refresh</button>
    </template>

    <p v-if="error" class="status error">{{ error }}</p>

    <section class="tweet-list">
      <MobileTweetCard
        v-for="item in items"
        :key="item.id"
        :tweet="item"
        time-field="publishedAt"
      />
      <p v-if="!loading && items.length === 0" class="status empty">No posts matched.</p>
    </section>

    <button v-if="nextCursor" class="load-more" type="button" :disabled="loading" @click="loadMore">
      Load more
    </button>
  </MobileShell>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'

import { fetchV2Tweets, type JsonRecord } from '../../../shared/api'
import MobileShell from '../components/MobileShell.vue'
import MobileTweetCard from '../components/MobileTweetCard.vue'

const include = 'author,stats,media,media-resources'
const items = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const loading = ref(false)
const error = ref('')

onMounted(reload)

async function reload() {
  await load(true)
}

async function loadMore() {
  await load(false)
}

async function load(reset: boolean) {
  loading.value = true
  error.value = ''
  try {
    const response = await fetchV2Tweets({
      relation: 'all',
      sort: 'publishedAt',
      include,
      cursor: reset ? undefined : nextCursor.value,
      limit: 25,
    })
    items.value = reset ? response.data : [...items.value, ...response.data]
    nextCursor.value = response.pagination.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load timeline'
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.tweet-list {
  display: grid;
}

.text-button,
.load-more {
  border: 1px solid #cfd9de;
  border-radius: 999px;
  background: #ffffff;
  color: #0f1419;
  font-weight: 750;
  cursor: pointer;
}

.text-button {
  min-height: 34px;
  padding: 0 13px;
  font-size: 0.86rem;
}

.load-more {
  width: calc(100% - 28px);
  min-height: 42px;
  margin: 14px 14px 20px;
}

.text-button:disabled,
.load-more:disabled {
  cursor: default;
  opacity: 0.55;
}

.status {
  margin: 12px 14px;
  padding: 10px 12px;
  border-radius: 8px;
}

.error {
  border: 1px solid #f5c2c7;
  background: #fff5f5;
  color: #9f1239;
}

.empty {
  color: #536471;
}
</style>
