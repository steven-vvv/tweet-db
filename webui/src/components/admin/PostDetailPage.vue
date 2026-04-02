<template>
  <section class="stack">
    <header class="page-head">
      <div>
        <p class="eyebrow">Post Detail</p>
        <h2>{{ summary.sourcePostId ?? route.params.sourcePostId }}</h2>
      </div>
      <RouterLink
        v-if="author?.source_actor_id || author?.sourceActorId"
        :to="`/admin/actors/${summary.sourceKind}/${author?.source_actor_id ?? author?.sourceActorId}`"
        class="pill-link"
      >
        Open author
      </RouterLink>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="summary-card">
      <div class="summary-row">
        <span>Source kind</span>
        <strong>{{ summary.sourceKind ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Author</span>
        <strong>{{ summary.authorSourceActorId ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Media count</span>
        <strong>{{ summary.mediaCount ?? 0 }}</strong>
      </div>
      <div class="summary-row">
        <span>Last observed</span>
        <strong>{{ summary.lastObservedAt ?? '-' }}</strong>
      </div>
    </section>

    <section class="panel">
      <h3>Linked media</h3>
      <div v-if="media.length === 0" class="hint">No linked media rows were found.</div>
      <RouterLink
        v-for="item in media"
        :key="item.managedMediaId"
        :to="`/admin/media/${item.managedMediaId}`"
        class="link-row"
      >
        <strong>{{ item.sourceMediaId }}</strong>
        <span>{{ item.mediaType }}</span>
        <span>{{ item.transferStatus ?? 'no transfer job' }}</span>
      </RouterLink>
    </section>

    <JsonPanel title="Post Record" :value="detail.record" />
    <JsonPanel title="Related" :value="detail.related" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

import { asArray, asRecord } from '../../admin-helpers'
import { fetchAdminPost, type DetailResponse } from '../../api'
import JsonPanel from './JsonPanel.vue'

const route = useRoute()
const detail = ref<DetailResponse>({ summary: {}, record: {}, related: {} })
const error = ref('')

const summary = computed(() => asRecord(detail.value.summary))
const related = computed(() => asRecord(detail.value.related))
const media = computed(() => asArray(related.value.media))
const author = computed(() => asRecord(related.value.author))

onMounted(() => {
  void load()
})

async function load() {
  error.value = ''

  try {
    detail.value = await fetchAdminPost(
      String(route.params.sourceKind),
      String(route.params.sourcePostId),
    )
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load post detail'
  }
}
</script>

<style scoped>
.stack,
.panel {
  display: grid;
  gap: 18px;
}

.page-head {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: end;
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

.link-row {
  padding: 12px 0;
  color: inherit;
  text-decoration: none;
  border-top: 1px solid #edf2f7;
}

.link-row:first-of-type {
  border-top: 0;
}

.pill-link {
  width: fit-content;
  padding: 11px 18px;
  border-radius: 999px;
  background: #10203a;
  color: white;
  text-decoration: none;
}
</style>
