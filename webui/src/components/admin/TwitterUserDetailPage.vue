<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>{{ displayName }}</h1>
        <p>{{ handle }}</p>
      </div>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="panel panel-pad kv">
      <div>
        <span>User ID</span>
        <strong class="mono">{{ summary.id ?? route.params.userId }}</strong>
      </div>
      <div>
        <span>Tweets</span>
        <strong>{{ countValue(summary.tweetCount) }}</strong>
      </div>
      <div>
        <span>Media</span>
        <strong>{{ countValue(summary.mediaCount) }}</strong>
      </div>
      <div>
        <span>Updated</span>
        <strong>{{ summary.updatedAt ?? '-' }}</strong>
      </div>
    </section>

    <section class="split">
      <section class="panel panel-pad">
        <h2>Recent tweets</h2>
        <RouterLink
          v-for="item in recentTweets"
          :key="item.id"
          :to="`/admin/tweets/${item.id}`"
          class="mini-row"
        >
          <strong>{{ item.text || item.id }}</strong>
          <span>{{ item.publishedAt }}</span>
        </RouterLink>
        <p v-if="recentTweets.length === 0" class="empty">No tweets.</p>
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
          <span>{{ item.type }} · {{ item.updatedAt }}</span>
        </RouterLink>
        <p v-if="media.length === 0" class="empty">No media.</p>
      </section>
    </section>

    <JsonPanel title="Profile Snapshot" :value="related.snapshot" />
    <JsonPanel title="Stats" :value="related.stats" />
    <JsonPanel title="User Record" :value="detail.record" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

import { asArray, asRecord, countValue, textValue } from '../../shared/admin-helpers'
import { fetchAdminTwitterUser, type DetailResponse } from '../../shared/api'
import JsonPanel from './JsonPanel.vue'

const route = useRoute()
const detail = ref<DetailResponse>({ summary: {}, record: {}, related: {} })
const error = ref('')

const summary = computed(() => asRecord(detail.value.summary))
const related = computed(() => asRecord(detail.value.related))
const snapshot = computed(() => asRecord(related.value.snapshot))
const recentTweets = computed(() => asArray(related.value.recentTweets))
const media = computed(() => asArray(related.value.media))
const displayName = computed(() => textValue(snapshot.value.display_name, String(route.params.userId)))
const handle = computed(() => (snapshot.value.user_name ? `@${snapshot.value.user_name}` : 'X user'))

onMounted(load)

async function load() {
  error.value = ''
  try {
    detail.value = await fetchAdminTwitterUser(String(route.params.userId))
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load X user'
  }
}
</script>

<style scoped>
h2 {
  margin: 0 0 10px;
  font-size: 1rem;
}

.split {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
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

@media (max-width: 900px) {
  .split {
    grid-template-columns: 1fr;
  }
}
</style>
