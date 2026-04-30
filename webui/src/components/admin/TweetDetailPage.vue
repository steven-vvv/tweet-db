<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>{{ summary.id ?? route.params.tweetId }}</h1>
        <p>{{ summary.publishedAt ?? 'Tweet detail' }}</p>
      </div>
      <RouterLink
        v-if="summary.authorId"
        class="button-link"
        :to="`/admin/twitter-users/${summary.authorId}`"
      >
        Author
      </RouterLink>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="panel panel-pad">
      <p class="tweet-text">{{ summary.text ?? '-' }}</p>
    </section>

    <section class="panel panel-pad kv">
      <div>
        <span>Tweet ID</span>
        <strong class="mono">{{ summary.id ?? '-' }}</strong>
      </div>
      <div>
        <span>Author ID</span>
        <strong class="mono">{{ summary.authorId ?? '-' }}</strong>
      </div>
      <div>
        <span>Published</span>
        <strong>{{ summary.publishedAt ?? '-' }}</strong>
      </div>
      <div>
        <span>Media count</span>
        <strong>{{ media.length }}</strong>
      </div>
    </section>

    <section class="panel panel-pad">
      <h2>Media</h2>
      <RouterLink
        v-for="item in media"
        :key="item.id"
        :to="`/admin/media/${item.id}`"
        class="mini-row"
      >
        <strong>{{ item.id }}</strong>
        <span>{{ item.type }} · {{ item.transferStatus ?? 'no transfer' }}</span>
      </RouterLink>
      <p v-if="media.length === 0" class="empty">No media.</p>
    </section>

    <JsonPanel title="Stats" :value="related.stats" />
    <JsonPanel title="Policy" :value="related.policy" />
    <JsonPanel title="Community Note" :value="related.communityNote" />
    <JsonPanel title="Tweet Record" :value="detail.record" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

import { asArray, asRecord } from '../../shared/admin-helpers'
import { fetchAdminTweet, type DetailResponse } from '../../shared/api'
import JsonPanel from './JsonPanel.vue'

const route = useRoute()
const detail = ref<DetailResponse>({ summary: {}, record: {}, related: {} })
const error = ref('')

const summary = computed(() => asRecord(detail.value.summary))
const related = computed(() => asRecord(detail.value.related))
const media = computed(() => asArray(related.value.media))

onMounted(load)

async function load() {
  error.value = ''
  try {
    detail.value = await fetchAdminTweet(String(route.params.tweetId))
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load tweet'
  }
}
</script>

<style scoped>
h2 {
  margin: 0 0 10px;
  font-size: 1rem;
}

.tweet-text {
  margin: 0;
  white-space: pre-wrap;
  line-height: 1.55;
}

.mini-row {
  display: grid;
  gap: 3px;
  padding: 8px 0;
  border-top: 1px solid #edf1f6;
  color: inherit;
  text-decoration: none;
}

.mini-row:first-of-type {
  border-top: 0;
}

.mini-row span {
  color: #66748a;
  font-size: 0.82rem;
}
</style>
