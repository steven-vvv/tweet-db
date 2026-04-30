<template>
  <main class="detail-layout">
    <section class="detail">
      <header class="detail-head">
        <a href="/browse" class="back">Back</a>
        <div>
          <h1>{{ tweet.id ?? route.params.tweetId }}</h1>
          <p>{{ timeLabel(tweet.publishedAt) }}</p>
        </div>
      </header>

      <p v-if="error" class="error">{{ error }}</p>

      <article v-if="tweet.id" class="tweet-detail">
        <header class="author-line">
          <RouterLink class="avatar" :to="`/browse/users/${tweet.authorId}`">
            <img v-if="authorAvatar(tweet)" :src="authorAvatar(tweet)" alt="" />
            <span v-else>{{ authorDisplayName(tweet).slice(0, 1).toUpperCase() }}</span>
          </RouterLink>
          <div>
            <RouterLink class="name" :to="`/browse/users/${tweet.authorId}`">
              {{ authorDisplayName(tweet) }}
            </RouterLink>
            <p>{{ authorUserName(tweet) }}</p>
          </div>
        </header>

        <TweetText class="detail-text" :text="tweetText(tweet, true)" />
        <TweetMediaGrid v-if="media.length > 0" :media="media" />

        <dl class="metrics">
          <div>
            <dt>Views</dt>
            <dd>{{ countValue(stats.views) }}</dd>
          </div>
          <div>
            <dt>Replies</dt>
            <dd>{{ countValue(stats.replies) }}</dd>
          </div>
          <div>
            <dt>Reposts</dt>
            <dd>{{ countValue(stats.reposts) }}</dd>
          </div>
          <div>
            <dt>Likes</dt>
            <dd>{{ countValue(stats.likes) }}</dd>
          </div>
          <div>
            <dt>Bookmarks</dt>
            <dd>{{ countValue(stats.bookmarks) }}</dd>
          </div>
        </dl>
      </article>
    </section>

    <aside class="facts">
      <section>
        <h2>Tweet</h2>
        <dl>
          <div>
            <dt>ID</dt>
            <dd class="mono">{{ textValue(tweet.id) }}</dd>
          </div>
          <div>
            <dt>Author</dt>
            <dd class="mono">{{ textValue(tweet.authorId) }}</dd>
          </div>
          <div>
            <dt>Relation</dt>
            <dd>{{ relationLabel(tweet) }}</dd>
          </div>
          <div>
            <dt>Media</dt>
            <dd>{{ countValue(media.length) }}</dd>
          </div>
        </dl>
      </section>

      <section v-if="hasPolicy">
        <h2>Policy</h2>
        <pre>{{ JSON.stringify(included.policy, null, 2) }}</pre>
      </section>

      <section v-if="hasCommunityNote">
        <h2>Community Note</h2>
        <pre>{{ JSON.stringify(included.communityNote, null, 2) }}</pre>
      </section>
    </aside>
  </main>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

import { fetchV2Tweet, type JsonRecord } from '../../../shared/api'
import {
  asArray,
  asRecord,
  authorAvatar,
  authorDisplayName,
  authorUserName,
  countValue,
  latestStats,
  mediaItems,
  relationLabel,
  textValue,
  timeLabel,
  tweetText,
} from '../browse-helpers'
import TweetMediaGrid from '../components/TweetMediaGrid.vue'
import TweetText from '../components/TweetText.vue'

const route = useRoute()
const tweet = ref<JsonRecord>({})
const included = ref<JsonRecord>({})
const error = ref('')

const stats = computed(() => asRecord(included.value.latestStats ?? latestStats(tweet.value)))
const media = computed(() => asArray<JsonRecord>(included.value.media ?? mediaItems(tweet.value)))
const hasPolicy = computed(() => Boolean(included.value.policy))
const hasCommunityNote = computed(() => Boolean(included.value.communityNote))

onMounted(load)

async function load() {
  error.value = ''
  try {
    const response = await fetchV2Tweet(String(route.params.tweetId), {
      include: 'author,stats,edit,policy,community-note,media,media-resources',
    })
    tweet.value = {
      ...response.data,
      author: response.included?.author,
      latestStats: response.included?.latestStats,
      media: response.included?.media,
    }
    included.value = response.included ?? {}
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load tweet'
  }
}
</script>

<style scoped>
.detail-layout {
  min-height: 100vh;
  display: grid;
  grid-template-columns: minmax(520px, 760px) minmax(260px, 360px);
  justify-content: center;
  gap: 18px;
  padding: 0 18px 28px;
}

.detail {
  min-width: 0;
  border-right: 1px solid #dfe5ee;
  border-left: 1px solid #dfe5ee;
  background: #fff;
}

.detail-head {
  position: sticky;
  top: 0;
  z-index: 2;
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

.detail-head p,
.author-line p {
  color: #647084;
  font-size: 0.84rem;
}

.tweet-detail {
  display: grid;
  gap: 14px;
  padding: 16px;
}

.author-line {
  display: flex;
  align-items: center;
  gap: 10px;
}

.avatar {
  width: 46px;
  height: 46px;
  border-radius: 50%;
  overflow: hidden;
  display: grid;
  place-items: center;
  background: #dfe6ef;
  text-decoration: none;
  font-weight: 800;
}

.avatar img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.name {
  color: #111827;
  font-weight: 800;
  text-decoration: none;
}

.detail-text {
  font-size: 1.02rem;
}

.metrics {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 8px;
  padding-top: 10px;
  border-top: 1px solid #e8edf3;
}

.metrics div {
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

.facts {
  position: sticky;
  top: 0;
  height: 100vh;
  display: grid;
  align-content: start;
  gap: 12px;
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

.facts dl div {
  display: grid;
  gap: 2px;
}

.mono {
  overflow-wrap: anywhere;
  font-family: "IBM Plex Mono", ui-monospace, monospace;
  font-size: 0.78rem;
}

pre {
  max-height: 260px;
  overflow: auto;
  margin: 0;
  border-radius: 6px;
  padding: 10px;
  background: #f5f7fa;
  font-size: 0.76rem;
}

.error {
  margin: 12px 16px;
  padding: 10px 12px;
  border: 1px solid #f1b8b8;
  border-radius: 8px;
  background: #fff7f7;
  color: #a12231;
}

@media (max-width: 980px) {
  .detail-layout {
    grid-template-columns: 1fr;
    padding: 0;
  }

  .facts {
    position: static;
    height: auto;
    padding: 0 16px 18px;
  }

  .metrics {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
