<template>
  <main class="user-layout">
    <section class="profile">
      <header class="profile-nav">
        <a href="/browse" class="back">Back</a>
        <h1>{{ displayName }}</h1>
      </header>

      <p v-if="error" class="error">{{ error }}</p>

      <section class="profile-card">
        <img v-if="bannerUrl" class="banner" :src="bannerUrl" alt="" />
        <div class="identity">
          <img v-if="avatarUrl" class="avatar" :src="avatarUrl" alt="" />
          <div v-else class="avatar fallback">{{ displayName.slice(0, 1).toUpperCase() }}</div>
          <div>
            <h2>{{ displayName }}</h2>
            <p>{{ handle }}</p>
          </div>
        </div>
        <TweetText v-if="bio.text" class="bio" :text="bio" />
        <dl class="stats">
          <div>
            <dt>Followers</dt>
            <dd>{{ countValue(stats.followers) }}</dd>
          </div>
          <div>
            <dt>Following</dt>
            <dd>{{ countValue(stats.following) }}</dd>
          </div>
          <div>
            <dt>Tweets</dt>
            <dd>{{ countValue(stats.tweets) }}</dd>
          </div>
          <div>
            <dt>Likes</dt>
            <dd>{{ countValue(stats.likes) }}</dd>
          </div>
        </dl>
      </section>

      <section class="tweet-list">
        <header class="section-toolbar">
          <h2>Recent posts</h2>
          <button type="button" :disabled="loadingTweets" @click="reloadTweets">Refresh</button>
        </header>
        <TweetCard v-for="item in tweets" :key="item.id" :tweet="item" />
        <p v-if="!loadingTweets && tweets.length === 0" class="empty">No posts matched.</p>
        <button
          v-if="nextCursor"
          class="load-more"
          type="button"
          :disabled="loadingTweets"
          @click="loadMoreTweets"
        >
          Load more
        </button>
      </section>
    </section>

    <aside class="facts">
      <section>
        <h2>User</h2>
        <dl>
          <div>
            <dt>ID</dt>
            <dd class="mono">{{ textValue(user.id ?? route.params.userId) }}</dd>
          </div>
          <div>
            <dt>Registered</dt>
            <dd>{{ timeLabel(user.registeredAt) }}</dd>
          </div>
          <div>
            <dt>Updated</dt>
            <dd>{{ timeLabel(user.updatedAt) }}</dd>
          </div>
        </dl>
      </section>
    </aside>
  </main>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'

import { fetchV2Tweets, fetchV2TwitterUser, type JsonRecord } from '../../../shared/api'
import {
  asRecord,
  countValue,
  optionalText,
  textValue,
  timeLabel,
} from '../browse-helpers'
import TweetCard from '../components/TweetCard.vue'
import TweetText from '../components/TweetText.vue'

const route = useRoute()
const user = ref<JsonRecord>({})
const included = ref<JsonRecord>({})
const tweets = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const loadingTweets = ref(false)
const error = ref('')

const snapshot = computed(() => asRecord(included.value.latestSnapshot))
const stats = computed(() => asRecord(included.value.latestStats))
const displayName = computed(() =>
  textValue(snapshot.value.display_name ?? snapshot.value.displayName ?? snapshot.value.user_name ?? snapshot.value.userName ?? route.params.userId),
)
const handle = computed(() => {
  const value = optionalText(snapshot.value.user_name ?? snapshot.value.userName)
  return value ? `@${value}` : textValue(route.params.userId)
})
const avatarUrl = computed(() => optionalText(snapshot.value.avatar_url ?? snapshot.value.avatarUrl))
const bannerUrl = computed(() => optionalText(snapshot.value.banner_url ?? snapshot.value.bannerUrl))
const bio = computed(() => asRecord(snapshot.value.bio))

onMounted(async () => {
  await loadUser()
  await reloadTweets()
})

async function loadUser() {
  error.value = ''
  try {
    const response = await fetchV2TwitterUser(String(route.params.userId), {
      include: 'latest-snapshot,latest-stats',
    })
    user.value = response.data
    included.value = response.included ?? {}
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load user'
  }
}

async function reloadTweets() {
  await loadTweets(true)
}

async function loadMoreTweets() {
  await loadTweets(false)
}

async function loadTweets(reset: boolean) {
  loadingTweets.value = true
  try {
    const response = await fetchV2Tweets({
      authorId: String(route.params.userId),
      relation: 'all',
      include: 'author,stats,media,media-resources',
      cursor: reset ? undefined : nextCursor.value,
      limit: 20,
    })
    tweets.value = reset ? response.data : [...tweets.value, ...response.data]
    nextCursor.value = response.pagination.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load posts'
  } finally {
    loadingTweets.value = false
  }
}
</script>

<style scoped>
.user-layout {
  min-height: 100vh;
  display: grid;
  grid-template-columns: minmax(520px, 760px) minmax(260px, 340px);
  justify-content: center;
  gap: 18px;
  padding: 0 18px 28px;
}

.profile {
  min-width: 0;
  border-right: 1px solid #dfe5ee;
  border-left: 1px solid #dfe5ee;
  background: #fff;
}

.profile-nav {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  border-bottom: 1px solid #dfe5ee;
  background: rgba(255, 255, 255, 0.96);
}

.back {
  border-radius: 999px;
  padding: 6px 10px;
  background: #eef2f7;
  text-decoration: none;
  font-weight: 700;
}

h1,
h2,
p,
dl {
  margin: 0;
}

h1 {
  font-size: 1.12rem;
}

.profile-card {
  display: grid;
  gap: 12px;
  padding-bottom: 14px;
  border-bottom: 1px solid #dfe5ee;
}

.banner {
  width: 100%;
  height: 180px;
  object-fit: cover;
  background: #dfe6ef;
}

.identity {
  display: flex;
  align-items: end;
  gap: 12px;
  padding: 0 16px;
}

.avatar {
  width: 76px;
  height: 76px;
  margin-top: -44px;
  border: 3px solid #fff;
  border-radius: 50%;
  object-fit: cover;
  background: #dfe6ef;
}

.fallback {
  display: grid;
  place-items: center;
  color: #334155;
  font-size: 1.5rem;
  font-weight: 800;
}

.identity h2 {
  font-size: 1.15rem;
}

.identity p {
  color: #647084;
}

.bio {
  padding: 0 16px;
}

.stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
  padding: 0 16px;
}

.stats div {
  display: grid;
  gap: 2px;
}

dt {
  color: #647084;
  font-size: 0.76rem;
}

dd {
  margin: 0;
  font-weight: 800;
}

.section-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid #e8edf3;
}

button {
  border: 1px solid #cbd5e1;
  border-radius: 6px;
  background: #fff;
  color: #172033;
  padding: 8px 10px;
  font: inherit;
  cursor: pointer;
}

button:disabled {
  cursor: default;
  opacity: 0.55;
}

.load-more {
  width: calc(100% - 32px);
  margin: 14px 16px 20px;
}

.facts {
  position: sticky;
  top: 0;
  height: 100vh;
  padding: 18px 0;
}

.facts section {
  display: grid;
  gap: 10px;
  border: 1px solid #dfe5ee;
  border-radius: 8px;
  padding: 12px;
  background: #fff;
}

.facts dl {
  display: grid;
  gap: 8px;
}

.mono {
  overflow-wrap: anywhere;
  font-family: "IBM Plex Mono", ui-monospace, monospace;
  font-size: 0.78rem;
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

@media (max-width: 980px) {
  .user-layout {
    grid-template-columns: 1fr;
    padding: 0;
  }

  .facts {
    position: static;
    height: auto;
    padding: 0 16px 18px;
  }
}
</style>
