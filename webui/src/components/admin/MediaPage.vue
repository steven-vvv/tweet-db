<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>Media</h1>
        <p>Tweet media, latest resources, and transfer status.</p>
      </div>
    </header>

    <form class="toolbar" @submit.prevent="reload">
      <input v-model="q" type="search" placeholder="Media, tweet, or user ID prefix" />
      <select v-model="mediaType">
        <option value="all">All types</option>
        <option value="photo">Photo</option>
        <option value="video">Video</option>
        <option value="animated_gif">Animated GIF</option>
      </select>
      <select v-model="transferStatus">
        <option value="all">All transfers</option>
        <option value="pending">Pending</option>
        <option value="processing">Processing</option>
        <option value="completed">Completed</option>
        <option value="failed">Failed</option>
        <option value="canceled">Canceled</option>
      </select>
      <button type="submit" class="primary" :disabled="loading">Search</button>
    </form>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="table">
      <div class="table-head cols">
        <span>Media</span>
        <span>Type</span>
        <span>Transfer</span>
        <span>Origin</span>
        <span>Updated</span>
      </div>
      <RouterLink
        v-for="item in items"
        :key="item.id"
        :to="`/admin/media/${item.id}`"
        class="table-row cols"
      >
        <div>
          <strong class="mono">{{ item.id }}</strong>
          <div class="muted">{{ item.altText ?? item.storageObjectKey ?? '-' }}</div>
        </div>
        <span>{{ item.type }}</span>
        <span class="badge" :class="statusTone(item.transferStatus)">
          {{ item.transferStatus ?? 'none' }}
        </span>
        <span class="mono">{{ item.originTweetId ?? item.originUserId ?? '-' }}</span>
        <span class="muted">{{ item.updatedAt }}</span>
      </RouterLink>
      <p v-if="!loading && items.length === 0" class="empty">No media matched.</p>
    </section>

    <button v-if="nextCursor" type="button" :disabled="loading" @click="loadMore">
      Load more
    </button>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'

import { statusTone } from '../../shared/admin-helpers'
import { fetchAdminMediaList, type JsonRecord } from '../../shared/api'

const items = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const q = ref('')
const mediaType = ref('all')
const transferStatus = ref('all')
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
    const response = await fetchAdminMediaList({
      q: q.value.trim() || undefined,
      mediaType: mediaType.value,
      transferStatus: transferStatus.value,
      cursor: reset ? undefined : nextCursor.value,
    })
    items.value = reset ? response.items : [...items.value, ...response.items]
    nextCursor.value = response.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load media'
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.cols {
  grid-template-columns: minmax(220px, 1.6fr) 120px 110px minmax(150px, 1fr) 220px;
}

@media (max-width: 980px) {
  .cols {
    grid-template-columns: 1fr;
  }
}
</style>
