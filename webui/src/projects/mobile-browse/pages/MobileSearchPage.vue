<template>
  <MobileShell title="Search">
    <template #action>
      <button
        class="top-action-button"
        type="button"
        :aria-expanded="searchPanelOpen"
        aria-controls="mobile-search-panel"
        @click="toggleSearchPanel"
      >
        {{ searchPanelOpen ? 'Hide' : 'Search' }}
      </button>
    </template>

    <section v-if="searchPanelOpen" id="mobile-search-panel" class="search-panel">
      <form class="search-form" @submit.prevent="submitSearch">
        <input
          v-model="form.q"
          type="search"
          placeholder="Search saved posts"
          enterkeyhint="search"
        />
        <input v-model="form.authorId" type="text" placeholder="Author ID" />
        <div class="field-row">
          <select v-model="form.sort">
            <option value="relevance" :disabled="!hasQuery">Relevance</option>
            <option value="publishedAt">Posted</option>
            <option value="createdAt">Saved</option>
            <option value="updatedAt">Updated</option>
          </select>
          <select v-model="form.relation">
            <option value="all">All</option>
            <option value="original">Original</option>
            <option value="reply">Reply</option>
            <option value="quote">Quote</option>
            <option value="repost">Repost</option>
          </select>
        </div>
        <button type="submit" :disabled="loading">Search</button>
      </form>
    </section>

    <p v-if="error" class="status error">{{ error }}</p>

    <section class="tweet-list">
      <MobileTweetCard
        v-for="item in items"
        :key="item.id"
        :tweet="item"
        :time-field="cardTimeField"
      />
      <p v-if="!loading && searched && items.length === 0" class="status empty">No posts matched.</p>
    </section>

    <button v-if="nextCursor" class="load-more" type="button" :disabled="loading" @click="loadMore">
      Load more
    </button>
  </MobileShell>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import {
  fetchV2TweetSearch,
  fetchV2Tweets,
  type JsonRecord,
} from '../../../shared/api'
import MobileShell from '../components/MobileShell.vue'
import MobileTweetCard from '../components/MobileTweetCard.vue'

type TweetSort = 'relevance' | 'publishedAt' | 'createdAt' | 'updatedAt'
type TweetTimeField = 'publishedAt' | 'createdAt' | 'updatedAt'

type SearchForm = {
  q: string
  authorId: string
  relation: string
  sort: TweetSort
}

const include = 'author,stats,media,media-resources'
const route = useRoute()
const router = useRouter()
const items = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const loading = ref(false)
const error = ref('')
const searched = ref(false)
const searchPanelOpen = ref(true)
const form = reactive<SearchForm>({
  q: '',
  authorId: '',
  relation: 'all',
  sort: 'relevance',
})

const hasQuery = computed(() => form.q.trim() !== '')
const requestSort = computed<TweetSort>(() => {
  if (!hasQuery.value && form.sort === 'relevance') {
    return 'publishedAt'
  }
  return form.sort
})
const cardTimeField = computed<TweetTimeField>(() =>
  requestSort.value === 'relevance' ? 'publishedAt' : requestSort.value,
)

onMounted(async () => {
  syncFormFromRoute()
  if (hasActiveFilters()) {
    searchPanelOpen.value = false
    await load(true)
  }
})

watch(
  () => route.query,
  async () => {
    syncFormFromRoute()
    if (hasActiveFilters()) {
      searchPanelOpen.value = false
      await load(true)
    } else {
      items.value = []
      nextCursor.value = null
      searched.value = false
      searchPanelOpen.value = true
    }
  },
)

watch(
  () => form.q,
  (next, previous) => {
    const nextActive = next.trim() !== ''
    const previousActive = previous.trim() !== ''
    if (nextActive && !previousActive) {
      form.sort = 'relevance'
    }
    if (!nextActive && previousActive && form.sort === 'relevance') {
      form.sort = 'publishedAt'
    }
  },
)

async function submitSearch() {
  await router.replace({
    path: '/mobile/browse/search',
    query: queryFromForm(),
  })
  if (hasActiveFilters()) {
    searchPanelOpen.value = false
  }
}

async function loadMore() {
  await load(false)
}

function toggleSearchPanel() {
  searchPanelOpen.value = !searchPanelOpen.value
}

async function load(reset: boolean) {
  loading.value = true
  error.value = ''
  searched.value = true
  try {
    const query = form.q.trim()
    const params = {
      authorId: form.authorId.trim() || undefined,
      relation: form.relation,
      sort: requestSort.value,
      include,
      cursor: reset ? undefined : nextCursor.value,
      limit: 25,
    }
    const response = query
      ? await fetchV2TweetSearch({
          ...params,
          q: query,
        })
      : await fetchV2Tweets(params)
    items.value = reset ? response.data : [...items.value, ...response.data]
    nextCursor.value = response.pagination.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load search results'
  } finally {
    loading.value = false
  }
}

function syncFormFromRoute() {
  form.q = queryText('q')
  form.authorId = queryText('authorId')
  form.relation = queryText('relation') || 'all'
  form.sort = querySort(queryText('sort') || (form.q ? 'relevance' : 'publishedAt'))
}

function queryFromForm(): Record<string, string> {
  const query: Record<string, string> = {}
  const q = form.q.trim()
  const authorId = form.authorId.trim()
  if (q) {
    query.q = q
  }
  if (authorId) {
    query.authorId = authorId
  }
  if (form.relation !== 'all') {
    query.relation = form.relation
  }
  if (requestSort.value !== 'publishedAt' || q) {
    query.sort = requestSort.value
  }
  return query
}

function hasActiveFilters(): boolean {
  return (
    form.q.trim() !== '' ||
    form.authorId.trim() !== '' ||
    form.relation !== 'all' ||
    requestSort.value !== 'publishedAt'
  )
}

function queryText(key: string): string {
  const value = route.query[key]
  return typeof value === 'string' ? value : ''
}

function querySort(value: string): TweetSort {
  if (
    value === 'relevance' ||
    value === 'publishedAt' ||
    value === 'createdAt' ||
    value === 'updatedAt'
  ) {
    return value
  }
  return hasQuery.value ? 'relevance' : 'publishedAt'
}
</script>

<style scoped>
.search-panel {
  position: sticky;
  top: 49px;
  z-index: 10;
  padding: 10px 14px;
  border-bottom: 1px solid #eff3f4;
  background: rgba(255, 255, 255, 0.96);
  backdrop-filter: blur(14px);
}

.top-action-button {
  min-height: 34px;
  border: 1px solid #cfd9de;
  border-radius: 999px;
  padding: 0 13px;
  background: #ffffff;
  color: #0f1419;
  font-size: 0.86rem;
  font-weight: 750;
  cursor: pointer;
}

.search-form {
  display: grid;
  gap: 9px;
}

.field-row {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 9px;
}

input,
select {
  min-width: 0;
  min-height: 42px;
  border: 1px solid #cfd9de;
  border-radius: 999px;
  padding: 0 14px;
  background: #f7f9f9;
  color: #0f1419;
  outline: none;
}

input:focus,
select:focus {
  border-color: #1d9bf0;
  background: #ffffff;
}

button {
  min-height: 42px;
  border: 1px solid #0f1419;
  border-radius: 999px;
  background: #0f1419;
  color: #ffffff;
  font-weight: 800;
  cursor: pointer;
}

button:disabled {
  cursor: default;
  opacity: 0.55;
}

.tweet-list {
  display: grid;
}

.load-more {
  width: calc(100% - 28px);
  margin: 14px 14px 20px;
  background: #ffffff;
  color: #0f1419;
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

@media (max-width: 380px) {
  .field-row {
    grid-template-columns: 1fr;
  }
}
</style>
