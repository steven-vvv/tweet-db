<template>
  <MobileBackShell title="Post">
    <p v-if="error" class="status error">{{ error }}</p>

    <article v-if="tweet.id" class="tweet-detail">
      <header class="author-line">
        <RouterLink class="avatar" :to="`${basePath}/users/${tweet.authorId}`">
          <img v-if="authorAvatar(tweet)" :src="authorAvatar(tweet)" alt="" />
          <span v-else>{{ authorDisplayName(tweet).slice(0, 1).toUpperCase() }}</span>
        </RouterLink>
        <div class="author-text">
          <RouterLink class="name" :to="`${basePath}/users/${tweet.authorId}`">
            {{ authorDisplayName(tweet) }}
          </RouterLink>
          <p>{{ authorUserName(tweet) }}</p>
        </div>
      </header>

      <TweetText
        class="detail-text"
        :text="tweetText(tweet, true)"
        :hide-media-entities="media.length > 0"
        :route-base-path="basePath"
      />
      <TweetMediaGrid v-if="media.length > 0" :media="media" />

      <p class="posted-at">{{ timeLabel(tweet.publishedAt) }}</p>

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
  </MobileBackShell>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
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
  timeLabel,
  tweetText,
} from '../../browse/browse-helpers'
import TweetMediaGrid from '../../browse/components/TweetMediaGrid.vue'
import TweetText from '../../browse/components/TweetText.vue'
import MobileBackShell from '../components/MobileBackShell.vue'

const basePath = '/mobile/browse'
const route = useRoute()
const tweet = ref<JsonRecord>({})
const included = ref<JsonRecord>({})
const error = ref('')

const stats = computed(() => asRecord(included.value.latestStats ?? latestStats(tweet.value)))
const media = computed(() => asArray<JsonRecord>(included.value.media ?? mediaItems(tweet.value)))

onMounted(load)

watch(
  () => route.params.tweetId,
  () => {
    load()
  },
)

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
.tweet-detail {
  display: grid;
  gap: 14px;
  padding: 16px 14px 24px;
}

.author-line {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.avatar {
  width: 46px;
  height: 46px;
  flex: 0 0 auto;
  display: grid;
  place-items: center;
  overflow: hidden;
  border-radius: 50%;
  background: #d8e0e8;
  color: #536471;
  font-weight: 800;
  text-decoration: none;
}

.avatar img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.author-text {
  min-width: 0;
}

.name {
  display: block;
  overflow: hidden;
  color: #0f1419;
  font-weight: 800;
  text-decoration: none;
  text-overflow: ellipsis;
  white-space: nowrap;
}

p,
dl {
  margin: 0;
}

.author-text p,
.posted-at {
  color: #536471;
  font-size: 0.88rem;
}

.detail-text {
  font-size: 1.08rem;
}

.metrics {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 9px;
  padding-top: 12px;
  border-top: 1px solid #eff3f4;
}

.metrics div {
  display: grid;
  gap: 2px;
}

dt {
  color: #536471;
  font-size: 0.76rem;
}

dd {
  margin: 0;
  color: #0f1419;
  font-weight: 800;
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
</style>
