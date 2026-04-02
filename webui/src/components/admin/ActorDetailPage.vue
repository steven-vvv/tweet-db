<template>
  <section class="stack">
    <header class="page-head">
      <div>
        <p class="eyebrow">Actor Detail</p>
        <h2>{{ summary.screenName || summary.sourceActorId || route.params.sourceActorId }}</h2>
      </div>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="summary-card">
      <div class="summary-row">
        <span>Source kind</span>
        <strong>{{ summary.sourceKind ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Actor ID</span>
        <strong>{{ summary.sourceActorId ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Name</span>
        <strong>{{ summary.name ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Last observed</span>
        <strong>{{ summary.lastObservedAt ?? '-' }}</strong>
      </div>
    </section>

    <section class="panel">
      <h3>Linked media</h3>
      <div class="media-links">
        <RouterLink
          v-if="summary.avatarMediaId"
          :to="`/admin/media/${summary.avatarMediaId}`"
          class="pill-link"
        >
          Avatar media
        </RouterLink>
        <RouterLink
          v-if="summary.bannerMediaId"
          :to="`/admin/media/${summary.bannerMediaId}`"
          class="pill-link"
        >
          Banner media
        </RouterLink>
      </div>
    </section>

    <section class="panel">
      <h3>Recent posts</h3>
      <RouterLink
        v-for="item in recentPosts"
        :key="item.source_post_id"
        :to="`/admin/posts/${item.source_kind}/${item.source_post_id}`"
        class="link-row"
      >
        <strong>{{ item.source_post_id }}</strong>
        <span>{{ item.last_observed_at }}</span>
      </RouterLink>
      <div v-if="recentPosts.length === 0" class="hint">No recent posts were linked.</div>
    </section>

    <JsonPanel title="Actor Record" :value="detail.record" />
    <JsonPanel title="Related" :value="detail.related" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

import { asArray, asRecord } from '../../admin-helpers'
import { fetchAdminActor, type DetailResponse } from '../../api'
import JsonPanel from './JsonPanel.vue'

const route = useRoute()
const detail = ref<DetailResponse>({ summary: {}, record: {}, related: {} })
const error = ref('')

const summary = computed(() => asRecord(detail.value.summary))
const related = computed(() => asRecord(detail.value.related))
const recentPosts = computed(() => asArray(related.value.recentPosts))

onMounted(() => {
  void load()
})

async function load() {
  error.value = ''

  try {
    detail.value = await fetchAdminActor(
      String(route.params.sourceKind),
      String(route.params.sourceActorId),
    )
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load actor detail'
  }
}
</script>

<style scoped>
.stack,
.panel {
  display: grid;
  gap: 18px;
}

.page-head h2,
.panel h3 {
  margin: 0;
}

.eyebrow {
  margin: 0;
  font-size: 0.74rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: #5e7698;
}

.error,
.hint {
  margin: 0;
}

.error {
  color: #a12231;
}

.hint {
  color: #516781;
}

.summary-card,
.panel {
  padding: 18px;
  border: 1px solid #d3ddeb;
  border-radius: 18px;
  background: white;
}

.summary-row,
.link-row {
  display: flex;
  justify-content: space-between;
  gap: 18px;
}

.media-links {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
}

.pill-link,
.link-row {
  color: inherit;
  text-decoration: none;
}

.pill-link {
  width: fit-content;
  padding: 11px 18px;
  border-radius: 999px;
  background: #10203a;
  color: white;
}

.link-row {
  padding: 12px 0;
  border-top: 1px solid #edf2f7;
}

.link-row:first-of-type {
  border-top: 0;
}
</style>
