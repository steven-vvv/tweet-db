<template>
  <MobileBackShell :title="displayName">
    <p v-if="error" class="status error">{{ error }}</p>

    <section class="profile-card">
      <img v-if="bannerUrl" class="banner" :src="bannerUrl" alt="" />
      <div v-else class="banner fallback-banner" />

      <div class="identity">
        <img v-if="avatarUrl" class="avatar" :src="avatarUrl" alt="" />
        <div v-else class="avatar fallback-avatar">{{ displayName.slice(0, 1).toUpperCase() }}</div>
        <div class="identity-text">
          <h2>{{ displayName }}</h2>
          <p>{{ handle }}</p>
        </div>
      </div>

      <TweetText
        v-if="bio.text"
        class="bio"
        :text="bio"
        :route-base-path="basePath"
      />

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
      <header class="section-head">
        <h2>Recent posts</h2>
        <button type="button" :disabled="loadingTweets" @click="reloadTweets">Refresh</button>
      </header>
      <MobileTweetCard v-for="item in tweets" :key="item.id" :tweet="item" />
      <p v-if="!loadingTweets && tweets.length === 0" class="status empty">No posts matched.</p>
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
  </MobileBackShell>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'

import { fetchV2TwitterUser, fetchV2TwitterUserTweets, type JsonRecord } from '../../../shared/api'
import {
  asRecord,
  countValue,
  optionalText,
  textValue,
} from '../../browse/browse-helpers'
import TweetText from '../../browse/components/TweetText.vue'
import MobileBackShell from '../components/MobileBackShell.vue'
import MobileTweetCard from '../components/MobileTweetCard.vue'

const basePath = '/mobile/browse'
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
  textValue(
    snapshot.value.display_name ??
      snapshot.value.displayName ??
      snapshot.value.user_name ??
      snapshot.value.userName ??
      route.params.userId,
  ),
)
const handle = computed(() => {
  const value = optionalText(snapshot.value.user_name ?? snapshot.value.userName)
  return value ? `@${value}` : textValue(route.params.userId)
})
const avatarUrl = computed(() => optionalText(snapshot.value.avatar_url ?? snapshot.value.avatarUrl))
const bannerUrl = computed(() => optionalText(snapshot.value.banner_url ?? snapshot.value.bannerUrl))
const bio = computed(() => asRecord(snapshot.value.bio))

onMounted(loadPage)

watch(
  () => route.params.userId,
  () => {
    loadPage()
  },
)

async function loadPage() {
  await loadUser()
  await reloadTweets()
}

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
    const response = await fetchV2TwitterUserTweets(String(route.params.userId), {
      relation: 'all',
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
.profile-card {
  display: grid;
  gap: 12px;
  padding-bottom: 14px;
  border-bottom: 1px solid #eff3f4;
}

.banner {
  width: 100%;
  height: 142px;
  object-fit: cover;
  background: #cfd9de;
}

.fallback-banner {
  background: linear-gradient(135deg, #cfd9de, #f7f9f9);
}

.identity {
  min-width: 0;
  display: flex;
  align-items: end;
  gap: 12px;
  padding: 0 14px;
}

.avatar {
  width: 76px;
  height: 76px;
  flex: 0 0 auto;
  margin-top: -44px;
  border: 3px solid #ffffff;
  border-radius: 50%;
  object-fit: cover;
  background: #d8e0e8;
}

.fallback-avatar {
  display: grid;
  place-items: center;
  color: #536471;
  font-size: 1.5rem;
  font-weight: 800;
}

.identity-text {
  min-width: 0;
}

h2,
p,
dl {
  margin: 0;
}

h2 {
  overflow: hidden;
  color: #0f1419;
  font-size: 1.16rem;
  font-weight: 800;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.identity-text p {
  color: #536471;
}

.bio {
  padding: 0 14px;
}

.stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
  padding: 0 14px;
}

.stats div {
  display: grid;
  gap: 2px;
}

dt {
  color: #536471;
  font-size: 0.74rem;
}

dd {
  margin: 0;
  color: #0f1419;
  font-weight: 800;
}

.tweet-list {
  display: grid;
}

.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 14px;
  border-bottom: 1px solid #eff3f4;
}

.section-head h2 {
  font-size: 1rem;
}

button {
  min-height: 36px;
  border: 1px solid #cfd9de;
  border-radius: 999px;
  background: #ffffff;
  color: #0f1419;
  padding: 0 13px;
  font-weight: 750;
  cursor: pointer;
}

button:disabled {
  cursor: default;
  opacity: 0.55;
}

.load-more {
  width: calc(100% - 28px);
  min-height: 42px;
  margin: 14px 14px 20px;
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
  .stats {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
