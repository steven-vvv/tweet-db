<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>Storage Objects</h1>
        <p>Objects created by media transfer tasks.</p>
      </div>
    </header>

    <form class="toolbar" @submit.prevent="reload">
      <input v-model="q" type="search" placeholder="Object key or object ID prefix" />
      <button type="submit" class="primary" :disabled="loading">Search</button>
    </form>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="table">
      <div class="table-head cols">
        <span>Object key</span>
        <span>Type</span>
        <span>Size</span>
        <span>Tasks</span>
        <span>Created</span>
      </div>
      <RouterLink
        v-for="item in items"
        :key="item.id"
        :to="`/admin/storage-objects/${item.id}`"
        class="table-row cols"
      >
        <div>
          <strong>{{ item.objectKey }}</strong>
          <div class="muted mono">{{ item.id }}</div>
        </div>
        <span>{{ item.contentType }}</span>
        <span>{{ countValue(item.contentLength) }}</span>
        <span>{{ countValue(item.taskCount) }}</span>
        <span class="muted">{{ item.createdAt }}</span>
      </RouterLink>
      <p v-if="!loading && items.length === 0" class="empty">No objects matched.</p>
    </section>

    <button v-if="nextCursor" type="button" :disabled="loading" @click="loadMore">
      Load more
    </button>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'

import { countValue } from '../../admin-helpers'
import { fetchAdminStorageObjects, type JsonRecord } from '../../api'

const items = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const q = ref('')
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
    const response = await fetchAdminStorageObjects({
      q: q.value.trim() || undefined,
      cursor: reset ? undefined : nextCursor.value,
    })
    items.value = reset ? response.items : [...items.value, ...response.items]
    nextCursor.value = response.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load objects'
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.cols {
  grid-template-columns: minmax(280px, 2fr) minmax(120px, 0.8fr) 110px 80px 220px;
}

@media (max-width: 980px) {
  .cols {
    grid-template-columns: 1fr;
  }
}
</style>
