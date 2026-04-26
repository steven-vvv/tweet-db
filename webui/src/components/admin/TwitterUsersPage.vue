<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>X Users</h1>
        <p>Saved X user profiles and activity counters.</p>
      </div>
    </header>

    <form class="toolbar" @submit.prevent="reload">
      <input v-model="q" type="search" placeholder="User ID, handle, or name prefix" />
      <button type="submit" class="primary" :disabled="loading">Search</button>
    </form>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="table">
      <div class="table-head cols">
        <span>User</span>
        <span>Followers</span>
        <span>Saved tweets</span>
        <span>Media</span>
        <span>Updated</span>
      </div>
      <RouterLink
        v-for="item in items"
        :key="item.id"
        :to="`/admin/twitter-users/${item.id}`"
        class="table-row cols"
      >
        <div>
          <strong>{{ item.displayName ?? item.userName ?? item.id }}</strong>
          <div class="muted">@{{ item.userName ?? item.id }}</div>
        </div>
        <span>{{ countValue(item.followers) }}</span>
        <span>{{ countValue(item.savedTweets) }}</span>
        <span>{{ countValue(item.savedMedia) }}</span>
        <span class="muted">{{ item.updatedAt }}</span>
      </RouterLink>
      <p v-if="!loading && items.length === 0" class="empty">No X users matched.</p>
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
import { fetchAdminTwitterUsers, type JsonRecord } from '../../api'

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
    const response = await fetchAdminTwitterUsers({
      q: q.value.trim() || undefined,
      cursor: reset ? undefined : nextCursor.value,
    })
    items.value = reset ? response.items : [...items.value, ...response.items]
    nextCursor.value = response.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load X users'
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.cols {
  grid-template-columns: minmax(220px, 1.6fr) 110px 120px 90px 220px;
}

@media (max-width: 900px) {
  .cols {
    grid-template-columns: 1fr;
  }
}
</style>
