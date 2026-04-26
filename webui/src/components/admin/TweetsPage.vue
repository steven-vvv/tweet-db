<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>Tweets</h1>
        <p>Recent saved tweets with structured filters.</p>
      </div>
    </header>

    <form class="toolbar" @submit.prevent="reload">
      <input v-model="q" type="search" placeholder="Tweet ID or author ID prefix" />
      <input v-model="authorId" type="text" placeholder="Author ID" />
      <select v-model="relation">
        <option value="all">All relations</option>
        <option value="original">Original</option>
        <option value="reply">Reply</option>
        <option value="quote">Quote</option>
        <option value="repost">Repost</option>
      </select>
      <button type="submit" class="primary" :disabled="loading">Search</button>
    </form>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="table">
      <div class="table-head cols">
        <span>Tweet</span>
        <span>Author</span>
        <span>Relation</span>
        <span>Media</span>
        <span>Published</span>
      </div>
      <RouterLink
        v-for="item in items"
        :key="item.id"
        :to="`/admin/tweets/${item.id}`"
        class="table-row cols"
      >
        <div>
          <strong>{{ item.text || item.id }}</strong>
          <div class="muted mono">{{ item.id }}</div>
        </div>
        <span>{{ item.authorUserName ?? item.authorId }}</span>
        <span class="badge">{{ relationLabel(item) }}</span>
        <span>{{ countValue(item.mediaCount) }}</span>
        <span class="muted">{{ item.publishedAt }}</span>
      </RouterLink>
      <p v-if="!loading && items.length === 0" class="empty">No tweets matched.</p>
    </section>

    <button v-if="nextCursor" type="button" :disabled="loading" @click="loadMore">
      Load more
    </button>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'

import { countValue, relationLabel } from '../../admin-helpers'
import { fetchAdminTweets, type JsonRecord } from '../../api'

const items = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const q = ref('')
const authorId = ref('')
const relation = ref('all')
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
    const response = await fetchAdminTweets({
      q: q.value.trim() || undefined,
      authorId: authorId.value.trim() || undefined,
      relation: relation.value,
      cursor: reset ? undefined : nextCursor.value,
    })
    items.value = reset ? response.items : [...items.value, ...response.items]
    nextCursor.value = response.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load tweets'
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.cols {
  grid-template-columns: minmax(260px, 2fr) minmax(120px, 0.8fr) 90px 80px 220px;
}

@media (max-width: 980px) {
  .cols {
    grid-template-columns: 1fr;
  }
}
</style>
