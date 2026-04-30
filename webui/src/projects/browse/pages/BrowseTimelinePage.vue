<template>
  <main class="browse-layout">
    <aside class="rail">
      <a class="brand" href="/browse">tweet-db</a>
      <nav>
        <a href="/browse" class="active">Browse</a>
        <a href="/admin/overview">Admin</a>
        <a href="/account">Account</a>
      </nav>
    </aside>

    <section class="feed">
      <p v-if="error" class="error">{{ error }}</p>

      <section class="tweet-list">
        <TweetCard v-for="item in items" :key="item.id" :tweet="item" />
        <p v-if="!loading && items.length === 0" class="empty">No posts matched.</p>
      </section>

      <button v-if="nextCursor" class="load-more" type="button" :disabled="loading" @click="loadMore">
        Load more
      </button>
    </section>

    <aside class="side-panel">
      <form class="search-panel" @submit.prevent="reload">
        <h1>Timeline</h1>
        <p>Latest saved posts</p>
        <input v-model="q" type="search" placeholder="Tweet ID or author ID prefix" />
        <input v-model="authorId" type="text" placeholder="Author ID" />
        <select v-model="relation">
          <option value="all">All relations</option>
          <option value="original">Original</option>
          <option value="reply">Reply</option>
          <option value="quote">Quote</option>
          <option value="repost">Repost</option>
        </select>
        <div class="panel-actions">
          <button type="submit" :disabled="loading">Search</button>
          <button type="button" :disabled="loading" @click="reload">Refresh</button>
        </div>
      </form>

      <section class="summary-panel">
        <h2>Status</h2>
        <dl>
          <div>
            <dt>Loaded</dt>
            <dd>{{ countValue(items.length) }}</dd>
          </div>
        </dl>
      </section>
    </aside>
  </main>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'

import { fetchV2Tweets, type JsonRecord } from '../../../shared/api'
import { countValue } from '../browse-helpers'
import TweetCard from '../components/TweetCard.vue'

const include = 'author,stats,media,media-resources'
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
    const response = await fetchV2Tweets({
      q: q.value.trim() || undefined,
      authorId: authorId.value.trim() || undefined,
      relation: relation.value,
      include,
      cursor: reset ? undefined : nextCursor.value,
      limit: 30,
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
.browse-layout {
  min-height: 100vh;
  display: grid;
  grid-template-columns: minmax(180px, 250px) minmax(520px, 760px) minmax(220px, 320px);
  justify-content: center;
  gap: 18px;
  padding: 0 18px;
}

.rail,
.side-panel {
  position: sticky;
  top: 0;
  height: 100vh;
  padding: 18px 0;
}

.brand {
  display: block;
  margin-bottom: 18px;
  color: #111827;
  font-weight: 800;
  text-decoration: none;
}

nav {
  display: grid;
  gap: 4px;
}

nav a {
  width: fit-content;
  border-radius: 999px;
  padding: 8px 12px;
  color: #334155;
  text-decoration: none;
  font-weight: 650;
}

nav a:hover,
nav a.active {
  background: #e5ebf3;
}

.feed {
  min-width: 0;
  border-right: 1px solid #dfe5ee;
  border-left: 1px solid #dfe5ee;
  background: #fff;
  padding-top: 4px;
}

h1,
h2,
p {
  margin: 0;
}

h1 {
  font-size: 1.22rem;
}

h2 {
  font-size: 0.95rem;
}

.search-panel p {
  color: #647084;
  font-size: 0.82rem;
}

button,
input,
select {
  border: 1px solid #cbd5e1;
  border-radius: 6px;
  background: #fff;
  color: #172033;
  padding: 8px 10px;
  font: inherit;
}

button {
  cursor: pointer;
}

button:disabled {
  cursor: default;
  opacity: 0.55;
}

.search-panel {
  display: grid;
  gap: 9px;
  border: 1px solid #dfe5ee;
  border-radius: 8px;
  padding: 12px;
  background: #fff;
}

.tweet-list {
  display: grid;
}

.error,
.empty {
  margin: 12px 16px;
  padding: 10px 12px;
  border-radius: 8px;
}

.error {
  border: 1px solid #f1b8b8;
  background: #fff7f7;
  color: #a12231;
}

.empty {
  color: #647084;
}

.load-more {
  width: calc(100% - 32px);
  margin: 14px 16px 20px;
}

.summary-panel {
  display: grid;
  gap: 12px;
  border: 1px solid #dfe5ee;
  border-radius: 8px;
  padding: 12px;
  background: #fff;
}

.panel-actions {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

dl {
  display: grid;
  gap: 8px;
  margin: 0;
}

dl div {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}

dt {
  color: #647084;
}

dd {
  margin: 0;
  font-weight: 700;
}

@media (max-width: 1120px) {
  .browse-layout {
    grid-template-columns: 180px minmax(0, 760px);
  }

  .rail {
    grid-row: span 2;
  }

  .side-panel {
    position: static;
    grid-column: 2;
    grid-row: 1;
    height: auto;
  }

  .feed {
    grid-column: 2;
    grid-row: 2;
  }
}

@media (max-width: 780px) {
  .browse-layout {
    grid-template-columns: 1fr;
    padding: 0;
  }

  .rail {
    position: static;
    grid-row: auto;
    height: auto;
    padding: 10px 12px;
    border-bottom: 1px solid #dfe5ee;
    background: #fff;
  }

  .side-panel,
  .feed {
    grid-column: auto;
    grid-row: auto;
  }

  .side-panel {
    padding: 10px 12px;
  }

  nav {
    display: flex;
    flex-wrap: wrap;
  }
}
</style>
