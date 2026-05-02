<template>
  <main class="browse-layout">
    <aside class="rail">
      <a class="brand" href="/browse">tweet-db</a>
      <nav>
        <a href="/browse" class="active">Browse</a>
        <a v-if="isAdmin" href="/admin/overview">Admin</a>
        <a href="/account">Account</a>
      </nav>
    </aside>

    <section class="feed">
      <header class="feed-head">
        <div>
          <h1>{{ modeLabel }}</h1>
          <p>{{ modeDescription }}</p>
        </div>
        <button type="button" :disabled="loading" @click="reload">Refresh</button>
      </header>

      <p v-if="error" class="error">{{ error }}</p>

      <section class="tweet-list">
        <template v-if="searchKind === 'posts'">
          <TweetCard
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
            :to="`/browse/users/${item.id}`"
          >
            <img v-if="userAvatar(item)" :src="userAvatar(item)" alt="" />
            <span v-else class="user-fallback">{{ userDisplayName(item).slice(0, 1).toUpperCase() }}</span>
            <span class="user-copy">
              <strong>{{ userDisplayName(item) }}</strong>
              <small>{{ userHandle(item) }}</small>
            </span>
          </RouterLink>
        </template>
        <p v-if="!loading && searched && resultItems.length === 0" class="empty">No results matched.</p>
      </section>

      <button v-if="nextCursor" class="load-more" type="button" :disabled="loading" @click="loadMore">
        Load more
      </button>
    </section>

    <aside class="side-panel">
      <form class="search-panel" @submit.prevent="reload">
        <h1>Search</h1>
        <p>Saved posts and authors</p>
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
            @input="handleQueryInput"
          />
          <input v-model="postForm.tweetIds" type="text" placeholder="Tweet IDs" />
          <input v-model="postForm.authorIds" type="text" placeholder="Author IDs" />
          <input v-model="postForm.authorUserNames" type="text" placeholder="Author usernames" />
          <select v-model="postForm.sort" @change="reload">
            <option value="relevance" :disabled="!hasPostQuery">Relevance</option>
            <option value="publishedAt">Posted</option>
            <option value="createdAt">Saved</option>
            <option value="updatedAt">Updated</option>
          </select>
          <select v-model="postForm.relation">
            <option value="all">All relations</option>
            <option value="original">Original</option>
            <option value="reply">Reply</option>
            <option value="quote">Quote</option>
            <option value="repost">Repost</option>
          </select>
        </template>

        <template v-else>
          <input v-model="userForm.userIds" type="text" placeholder="User IDs" />
          <input v-model="userForm.userNamePrefix" type="search" placeholder="Username prefix" />
          <input v-model="userForm.displayNamePrefix" type="search" placeholder="Display name prefix" />
        </template>
        <div class="panel-actions">
          <button type="submit" :disabled="loading">Search</button>
          <button type="button" :disabled="loading" @click="reload">Refresh</button>
        </div>
      </form>

      <section class="summary-panel">
        <h2>Status</h2>
        <dl>
          <div>
            <dt>Mode</dt>
            <dd>{{ modeLabel }}</dd>
          </div>
          <div>
            <dt>Loaded</dt>
            <dd>{{ countValue(resultItems.length) }}</dd>
          </div>
          <div v-if="searchKind === 'posts'">
            <dt>Sort</dt>
            <dd>{{ sortLabel }}</dd>
          </div>
        </dl>
      </section>
    </aside>
  </main>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { RouterLink } from 'vue-router'

import {
  fetchV2Me,
  fetchV2TweetSearch,
  fetchV2UserSearch,
  type JsonRecord,
} from '../../../shared/api'
import { asRecord, countValue, optionalText, textValue } from '../browse-helpers'
import TweetCard from '../components/TweetCard.vue'

type TweetSort = 'relevance' | 'publishedAt' | 'createdAt' | 'updatedAt'
type TweetTimeField = 'publishedAt' | 'createdAt' | 'updatedAt'
type SearchKind = 'posts' | 'users'

const resultItems = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const searchKind = ref<SearchKind>('posts')
const queryWasActive = ref(false)
const loading = ref(false)
const error = ref('')
const isAdmin = ref(false)
const searched = ref(false)
const postForm = reactive({
  q: '',
  tweetIds: '',
  authorIds: '',
  authorUserNames: '',
  relation: 'all',
  sort: 'publishedAt' as TweetSort,
})
const userForm = reactive({
  userIds: '',
  userNamePrefix: '',
  displayNamePrefix: '',
})

const sortLabels: Record<TweetSort, string> = {
  relevance: 'Relevance',
  publishedAt: 'Posted',
  createdAt: 'Saved',
  updatedAt: 'Updated',
}
const tweetItems = computed(() => resultItems.value)
const userItems = computed(() => resultItems.value)
const hasPostQuery = computed(() => postForm.q.trim() !== '')
const sortLabel = computed(() => sortLabels[requestSort.value])
const modeLabel = computed(() => (searchKind.value === 'posts' ? 'Posts' : 'Users'))
const modeDescription = computed(() =>
  searchKind.value === 'posts' ? 'Post text and author filters' : 'Author lookup',
)
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
  await loadSession()
  await reload()
})

async function reload() {
  await load(true)
}

async function loadMore() {
  await load(false)
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
            limit: 30,
          })
        : await fetchV2UserSearch({
            userIds: trimmed(userForm.userIds) || undefined,
            userNamePrefix: trimmed(userForm.userNamePrefix) || undefined,
            displayNamePrefix: trimmed(userForm.displayNamePrefix) || undefined,
            cursor: reset ? undefined : nextCursor.value,
            limit: 30,
          })
    resultItems.value = reset ? response.data : [...resultItems.value, ...response.data]
    nextCursor.value = response.pagination.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load timeline'
  } finally {
    loading.value = false
  }
}

function handleQueryInput() {
  const nextActive = postForm.q.trim() !== ''
  if (nextActive && !queryWasActive.value) {
    postForm.sort = 'relevance'
  }
  if (!nextActive && queryWasActive.value && postForm.sort === 'relevance') {
    postForm.sort = 'publishedAt'
  }
  queryWasActive.value = nextActive
}

function setSearchKind(kind: SearchKind) {
  if (searchKind.value === kind) {
    return
  }
  searchKind.value = kind
  resultItems.value = []
  nextCursor.value = null
}

function trimmed(value: string): string {
  return value.trim()
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

async function loadSession() {
  try {
    const response = await fetchV2Me()
    isAdmin.value = Boolean(response.data.isAdmin)
  } catch {
    isAdmin.value = false
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
}

.feed-head {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 16px;
  border-bottom: 1px solid #dfe5ee;
  background: rgba(255, 255, 255, 0.96);
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

.feed-head p,
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

.mode-tabs {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px;
}

.mode-tabs button {
  min-height: 34px;
  border-radius: 999px;
  font-weight: 750;
}

.mode-tabs button.active {
  border-color: #172033;
  background: #172033;
  color: #fff;
}

.tweet-list {
  display: grid;
}

.user-result {
  min-width: 0;
  display: grid;
  grid-template-columns: 42px minmax(0, 1fr);
  gap: 10px;
  padding: 14px 16px;
  border-bottom: 1px solid #e1e6ee;
  background: #fff;
  text-decoration: none;
}

.user-result:hover {
  background: #f8fafc;
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
  background: #dfe6ef;
  color: #334155;
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
  color: #161a22;
}

.user-copy small {
  color: #647084;
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
