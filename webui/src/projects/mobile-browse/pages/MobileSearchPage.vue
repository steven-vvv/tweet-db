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
        <div class="mode-tabs">
          <button type="button" :class="{ active: searchKind === 'posts' }" @click="setSearchKind('posts')">
            Posts
          </button>
          <button type="button" :class="{ active: searchKind === 'users' }" @click="setSearchKind('users')">
            Users
          </button>
        </div>

        <template v-if="searchKind === 'posts'">
          <input
            v-model="postForm.q"
            type="search"
            placeholder="Post text"
            enterkeyhint="search"
          />
          <input v-model="postForm.tweetIds" type="text" placeholder="Tweet IDs" />
          <input v-model="postForm.authorIds" type="text" placeholder="Author IDs" />
          <input v-model="postForm.authorUserNames" type="text" placeholder="Author usernames" />
          <div class="field-row">
            <select v-model="postForm.sort">
              <option value="relevance" :disabled="!hasPostQuery">Relevance</option>
              <option value="publishedAt">Posted</option>
              <option value="createdAt">Saved</option>
              <option value="updatedAt">Updated</option>
            </select>
            <select v-model="postForm.relation">
              <option value="all">All</option>
              <option value="original">Original</option>
              <option value="reply">Reply</option>
              <option value="quote">Quote</option>
              <option value="repost">Repost</option>
            </select>
          </div>
        </template>

        <template v-else>
          <input v-model="userForm.userIds" type="text" placeholder="User IDs" />
          <input v-model="userForm.userNamePrefix" type="search" placeholder="Username prefix" />
          <input v-model="userForm.displayNamePrefix" type="search" placeholder="Display name prefix" />
        </template>
        <button type="submit" :disabled="loading">Search</button>
      </form>
    </section>

    <p v-if="error" class="status error">{{ error }}</p>

    <section class="result-list">
      <template v-if="searchKind === 'posts'">
        <MobileTweetCard
          v-for="item in tweetItems"
          :key="item.id"
          :tweet="item"
          :time-field="cardTimeField"
        />
      </template>
      <template v-else>
        <RouterLink
          v-for="item in userItems"
          :key="item.id"
          class="user-result"
          :to="`/mobile/browse/users/${item.id}`"
        >
          <img v-if="userAvatar(item)" :src="userAvatar(item)" alt="" />
          <span v-else class="user-fallback">{{ userDisplayName(item).slice(0, 1).toUpperCase() }}</span>
          <span class="user-copy">
            <strong>{{ userDisplayName(item) }}</strong>
            <small>{{ userHandle(item) }}</small>
          </span>
        </RouterLink>
      </template>
      <p v-if="!loading && searched && resultItems.length === 0" class="status empty">No results matched.</p>
    </section>

    <button v-if="nextCursor" class="load-more" type="button" :disabled="loading" @click="loadMore">
      Load more
    </button>
  </MobileShell>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'

import {
  fetchV2TweetSearch,
  fetchV2UserSearch,
  type JsonRecord,
} from '../../../shared/api'
import { asRecord, optionalText, textValue } from '../../browse/browse-helpers'
import MobileShell from '../components/MobileShell.vue'
import MobileTweetCard from '../components/MobileTweetCard.vue'

type TweetSort = 'relevance' | 'publishedAt' | 'createdAt' | 'updatedAt'
type TweetTimeField = 'publishedAt' | 'createdAt' | 'updatedAt'
type SearchKind = 'posts' | 'users'

type PostSearchForm = {
  q: string
  tweetIds: string
  authorIds: string
  authorUserNames: string
  relation: string
  sort: TweetSort
}

const route = useRoute()
const router = useRouter()
const resultItems = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const loading = ref(false)
const error = ref('')
const searched = ref(false)
const searchPanelOpen = ref(true)
const searchKind = ref<SearchKind>('posts')
const postForm = reactive<PostSearchForm>({
  q: '',
  tweetIds: '',
  authorIds: '',
  authorUserNames: '',
  relation: 'all',
  sort: 'relevance',
})
const userForm = reactive({
  userIds: '',
  userNamePrefix: '',
  displayNamePrefix: '',
})

const tweetItems = computed(() => resultItems.value)
const userItems = computed(() => resultItems.value)
const hasPostQuery = computed(() => postForm.q.trim() !== '')
const requestSort = computed<TweetSort>(() => {
  if (!hasPostQuery.value && postForm.sort === 'relevance') {
    return 'publishedAt'
  }
  return postForm.sort
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
      resultItems.value = []
      nextCursor.value = null
      searched.value = false
      searchPanelOpen.value = true
    }
  },
)

watch(
  () => postForm.q,
  (next, previous) => {
    const nextActive = next.trim() !== ''
    const previousActive = previous.trim() !== ''
    if (nextActive && !previousActive) {
      postForm.sort = 'relevance'
    }
    if (!nextActive && previousActive && postForm.sort === 'relevance') {
      postForm.sort = 'publishedAt'
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

function setSearchKind(kind: SearchKind) {
  if (searchKind.value === kind) {
    return
  }
  searchKind.value = kind
  resultItems.value = []
  nextCursor.value = null
}

async function load(reset: boolean) {
  loading.value = true
  error.value = ''
  searched.value = true
  try {
    const response =
      searchKind.value === 'posts'
        ? await fetchV2TweetSearch({
            q: trimmed(postForm.q) || undefined,
            tweetIds: trimmed(postForm.tweetIds) || undefined,
            authorIds: trimmed(postForm.authorIds) || undefined,
            authorUserNames: trimmed(postForm.authorUserNames) || undefined,
            relation: postForm.relation,
            sort: requestSort.value,
            cursor: reset ? undefined : nextCursor.value,
            limit: 25,
          })
        : await fetchV2UserSearch({
            userIds: trimmed(userForm.userIds) || undefined,
            userNamePrefix: trimmed(userForm.userNamePrefix) || undefined,
            displayNamePrefix: trimmed(userForm.displayNamePrefix) || undefined,
            cursor: reset ? undefined : nextCursor.value,
            limit: 25,
          })
    resultItems.value = reset ? response.data : [...resultItems.value, ...response.data]
    nextCursor.value = response.pagination.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load search results'
  } finally {
    loading.value = false
  }
}

function syncFormFromRoute() {
  searchKind.value = querySearchKind(queryText('type'))
  postForm.q = queryText('q')
  postForm.tweetIds = queryText('tweetIds')
  postForm.authorIds = queryText('authorIds')
  postForm.authorUserNames = queryText('authorUserNames')
  postForm.relation = queryText('relation') || 'all'
  postForm.sort = querySort(queryText('sort') || (postForm.q ? 'relevance' : 'publishedAt'))
  userForm.userIds = queryText('userIds')
  userForm.userNamePrefix = queryText('userNamePrefix')
  userForm.displayNamePrefix = queryText('displayNamePrefix')
}

function queryFromForm(): Record<string, string> {
  const query: Record<string, string> = { type: searchKind.value }
  if (searchKind.value === 'users') {
    addQuery(query, 'userIds', userForm.userIds)
    addQuery(query, 'userNamePrefix', userForm.userNamePrefix)
    addQuery(query, 'displayNamePrefix', userForm.displayNamePrefix)
    return query
  }

  addQuery(query, 'q', postForm.q)
  addQuery(query, 'tweetIds', postForm.tweetIds)
  addQuery(query, 'authorIds', postForm.authorIds)
  addQuery(query, 'authorUserNames', postForm.authorUserNames)
  if (postForm.relation !== 'all') {
    query.relation = postForm.relation
  }
  if (requestSort.value !== 'publishedAt' || postForm.q.trim()) {
    query.sort = requestSort.value
  }
  return query
}

function hasActiveFilters(): boolean {
  if (searchKind.value === 'users') {
    return (
      userForm.userIds.trim() !== '' ||
      userForm.userNamePrefix.trim() !== '' ||
      userForm.displayNamePrefix.trim() !== ''
    )
  }

  return (
    postForm.q.trim() !== '' ||
    postForm.tweetIds.trim() !== '' ||
    postForm.authorIds.trim() !== '' ||
    postForm.authorUserNames.trim() !== '' ||
    postForm.relation !== 'all' ||
    requestSort.value !== 'publishedAt'
  )
}

function addQuery(query: Record<string, string>, key: string, value: string) {
  const normalized = value.trim()
  if (normalized) {
    query[key] = normalized
  }
}

function trimmed(value: string): string {
  return value.trim()
}

function queryText(key: string): string {
  const value = route.query[key]
  return typeof value === 'string' ? value : ''
}

function querySearchKind(value: string): SearchKind {
  return value === 'users' ? 'users' : 'posts'
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
  return hasPostQuery.value ? 'relevance' : 'publishedAt'
}

function userSnapshot(user: JsonRecord): JsonRecord {
  return asRecord(user.latestSnapshot)
}

function userDisplayName(user: JsonRecord): string {
  const snapshot = userSnapshot(user)
  return textValue(snapshot.display_name ?? snapshot.displayName ?? snapshot.user_name ?? snapshot.userName ?? user.id)
}

function userHandle(user: JsonRecord): string {
  const snapshot = userSnapshot(user)
  const handle = optionalText(snapshot.user_name ?? snapshot.userName)
  return handle ? `@${handle}` : textValue(user.id)
}

function userAvatar(user: JsonRecord): string {
  const snapshot = userSnapshot(user)
  return optionalText(snapshot.avatar_url ?? snapshot.avatarUrl)
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

.mode-tabs {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px;
}

.mode-tabs button {
  min-height: 36px;
  border: 1px solid #cfd9de;
  background: #ffffff;
  color: #0f1419;
}

.mode-tabs button.active {
  border-color: #0f1419;
  background: #0f1419;
  color: #ffffff;
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

.result-list {
  display: grid;
}

.user-result {
  min-width: 0;
  display: grid;
  grid-template-columns: 42px minmax(0, 1fr);
  gap: 10px;
  padding: 12px 14px;
  border-bottom: 1px solid #eff3f4;
  background: #ffffff;
  text-decoration: none;
}

.user-result img,
.user-fallback {
  width: 42px;
  height: 42px;
  border-radius: 50%;
}

.user-result img {
  object-fit: cover;
}

.user-fallback {
  display: grid;
  place-items: center;
  background: #d8e0e8;
  color: #536471;
  font-weight: 800;
}

.user-copy {
  min-width: 0;
  display: grid;
  align-content: center;
  gap: 2px;
}

.user-copy strong,
.user-copy small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.user-copy strong {
  color: #0f1419;
}

.user-copy small {
  color: #536471;
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
