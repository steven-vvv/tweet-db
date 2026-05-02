<template>
  <article class="tweet-card">
    <RouterLink class="avatar-link" :to="`${basePath}/users/${tweet.authorId}`">
      <img v-if="avatar" :src="avatar" alt="" />
      <span v-else>{{ authorInitial }}</span>
    </RouterLink>

    <section class="tweet-main">
      <header class="tweet-head">
        <RouterLink class="name" :to="`${basePath}/users/${tweet.authorId}`">
          {{ authorDisplayName(tweet) }}
        </RouterLink>
        <span>{{ authorUserName(tweet) }}</span>
        <span>·</span>
        <RouterLink class="time" :to="`${basePath}/tweets/${tweet.id}`">
          {{ timeLabel(displayTime) }}
        </RouterLink>
      </header>

      <RouterLink class="text-link" :to="`${basePath}/tweets/${tweet.id}`">
        <TweetText
          :text="tweetText(tweet)"
          :max-lines="7"
          :hide-media-entities="media.length > 0"
          :route-base-path="basePath"
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
} from '../../browse/browse-helpers'
import TweetMediaGrid from '../../browse/components/TweetMediaGrid.vue'
import TweetText from '../../browse/components/TweetText.vue'

const basePath = '/mobile/browse'

const props = defineProps<{
  tweet: JsonRecord
  timeField?: 'publishedAt' | 'createdAt' | 'updatedAt'
}>()

const avatar = computed(() => authorAvatar(props.tweet))
const media = computed(() => mediaItems(props.tweet))
const stats = computed(() => latestStats(props.tweet))
const displayTime = computed(() => props.tweet[props.timeField ?? 'publishedAt'])
const authorInitial = computed(() => authorDisplayName(props.tweet).slice(0, 1).toUpperCase())
const hasLongText = computed(() => hasNoteText(props.tweet))
</script>

<style scoped>
.tweet-card {
  display: grid;
  grid-template-columns: 42px minmax(0, 1fr);
  gap: 10px;
  padding: 12px 14px;
  border-bottom: 1px solid #eff3f4;
  background: #ffffff;
}

.avatar-link {
  width: 42px;
  height: 42px;
  display: grid;
  place-items: center;
  overflow: hidden;
  border-radius: 50%;
  background: #d8e0e8;
  color: #536471;
  font-weight: 800;
  text-decoration: none;
}

.avatar-link img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.tweet-main {
  min-width: 0;
  display: grid;
  gap: 8px;
}

.tweet-head {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 4px;
  color: #536471;
  font-size: 0.86rem;
}

.tweet-head span,
.time,
.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.name {
  min-width: 0;
  color: #0f1419;
  font-weight: 800;
  text-decoration: none;
}

.time,
.text-link {
  text-decoration: none;
}

.show-more {
  display: inline-block;
  margin-top: 4px;
  color: #1d9bf0;
  font-size: 0.86rem;
  font-weight: 750;
}

.metrics {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  color: #536471;
  font-size: 0.78rem;
}

@media (max-width: 380px) {
  .tweet-card {
    grid-template-columns: 36px minmax(0, 1fr);
    padding-right: 12px;
    padding-left: 12px;
  }

  .avatar-link {
    width: 36px;
    height: 36px;
  }
}
</style>
