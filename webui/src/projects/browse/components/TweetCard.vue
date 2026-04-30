<template>
  <article class="tweet-card">
    <RouterLink class="avatar-link" :to="`/browse/users/${tweet.authorId}`">
      <img v-if="avatar" :src="avatar" alt="" loading="lazy" />
      <span v-else>{{ authorInitial }}</span>
    </RouterLink>

    <section class="tweet-main">
      <header class="tweet-meta">
        <div class="author">
          <RouterLink class="name" :to="`/browse/users/${tweet.authorId}`">
            {{ authorDisplayName(tweet) }}
          </RouterLink>
          <span>{{ authorUserName(tweet) }}</span>
          <span>·</span>
          <RouterLink class="time" :to="`/browse/tweets/${tweet.id}`">
            {{ timeLabel(tweet.publishedAt) }}
          </RouterLink>
        </div>
      </header>

      <RouterLink class="text-link" :to="`/browse/tweets/${tweet.id}`">
        <TweetText
          :text="tweetText(tweet)"
          :max-lines="8"
          :hide-media-entities="media.length > 0"
        />
        <span v-if="hasLongText" class="show-more">Show more</span>
      </RouterLink>

      <TweetMediaGrid v-if="media.length > 0" :media="media" />

      <footer class="metrics">
        <span>Views {{ countValue(stats.views) }}</span>
        <span>Replies {{ countValue(stats.replies) }}</span>
        <span>Reposts {{ countValue(stats.reposts) }}</span>
        <span>Likes {{ countValue(stats.likes) }}</span>
      </footer>
    </section>
  </article>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { RouterLink } from 'vue-router'

import type { JsonRecord } from '../../../shared/api'
import {
  authorAvatar,
  authorDisplayName,
  authorUserName,
  countValue,
  hasNoteText,
  latestStats,
  mediaItems,
  timeLabel,
  tweetText,
} from '../browse-helpers'
import TweetMediaGrid from './TweetMediaGrid.vue'
import TweetText from './TweetText.vue'

const props = defineProps<{
  tweet: JsonRecord
}>()

const avatar = computed(() => authorAvatar(props.tweet))
const media = computed(() => mediaItems(props.tweet))
const stats = computed(() => latestStats(props.tweet))
const authorInitial = computed(() => authorDisplayName(props.tweet).slice(0, 1).toUpperCase())
const hasLongText = computed(() => hasNoteText(props.tweet))
</script>

<style scoped>
.tweet-card {
  display: grid;
  grid-template-columns: 42px minmax(0, 1fr);
  gap: 10px;
  padding: 14px 16px;
  border-bottom: 1px solid #e1e6ee;
  background: #fff;
}

.tweet-card:hover {
  background: #f8fafc;
}

.avatar-link {
  width: 42px;
  height: 42px;
  border-radius: 50%;
  overflow: hidden;
  display: grid;
  place-items: center;
  background: #dfe6ef;
  color: #334155;
  text-decoration: none;
  font-weight: 700;
}

.avatar-link img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.tweet-main {
  min-width: 0;
  display: grid;
  gap: 9px;
}

.tweet-meta {
  min-width: 0;
  display: block;
}

.author {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 5px;
  color: #647084;
  font-size: 0.86rem;
}

.author span,
.time {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.name {
  min-width: 0;
  color: #161a22;
  font-weight: 700;
  text-decoration: none;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.time,
.text-link {
  text-decoration: none;
}

.show-more {
  display: inline-block;
  margin-top: 4px;
  color: #1d72d2;
  font-size: 0.86rem;
  font-weight: 750;
}

.metrics {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  color: #68758a;
  font-size: 0.78rem;
}
</style>
