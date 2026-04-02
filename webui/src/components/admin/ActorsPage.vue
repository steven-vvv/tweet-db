<template>
  <section class="stack">
    <header class="page-head">
      <div>
        <p class="eyebrow">Actor Browse</p>
        <h2>Actors</h2>
      </div>
      <p class="hint">Search current actor profiles and inspect the latest stored versions.</p>
    </header>

    <form class="toolbar" @submit.prevent="reload">
      <input v-model="q" type="search" placeholder="Search actor ID, screen name, or name" />
      <input v-model="sourceKind" type="text" placeholder="Source kind" />
      <button type="submit" class="primary" :disabled="loading">Search</button>
    </form>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="list-card">
      <RouterLink
        v-for="item in items"
        :key="`${item.sourceKind}:${item.sourceActorId}`"
        :to="`/admin/actors/${item.sourceKind}/${item.sourceActorId}`"
        class="list-row"
      >
        <div>
          <strong>{{ item.screenName || item.sourceActorId }}</strong>
          <p>{{ item.name || '(unnamed actor)' }}</p>
        </div>
        <div class="meta">
          <span>{{ item.sourceKind }}</span>
          <span>{{ item.sourceActorId }}</span>
          <span>{{ item.lastObservedAt }}</span>
        </div>
      </RouterLink>
      <p v-if="!loading && items.length === 0" class="empty">No actors matched the current filter.</p>
    </section>

    <button
      v-if="nextCursor"
      type="button"
      class="secondary"
      :disabled="loading"
      @click="loadMore"
    >
      Load more
    </button>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'

import { fetchAdminActors, type JsonRecord } from '../../api'

const items = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const q = ref('')
const sourceKind = ref('')
const loading = ref(false)
const error = ref('')

onMounted(() => {
  void reload()
})

async function reload() {
  await load(true)
}

async function loadMore() {
  await load(false)
}

async function load(reset: boolean) {
  loading.value = true
  error.value = ''

  try {
    const response = await fetchAdminActors({
      q: q.value.trim() || undefined,
      sourceKind: sourceKind.value.trim() || undefined,
      cursor: reset ? undefined : nextCursor.value,
    })
    items.value = reset ? response.items : [...items.value, ...response.items]
    nextCursor.value = response.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load actors'
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.stack {
  display: grid;
  gap: 18px;
}

.page-head h2 {
  margin: 4px 0 0;
  font-size: 1.8rem;
}

.eyebrow {
  margin: 0;
  font-size: 0.74rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: #5e7698;
}

.hint,
.error,
.empty,
.list-row p {
  margin: 0;
}

.hint,
.empty,
.list-row p {
  color: #516781;
}

.error {
  color: #a12231;
}

.toolbar {
  display: grid;
  gap: 12px;
}

.toolbar input {
  width: 100%;
  border: 1px solid #cbd6e2;
  border-radius: 14px;
  padding: 12px 14px;
  font: inherit;
}

.list-card {
  display: grid;
  gap: 1px;
  border: 1px solid #d3ddeb;
  border-radius: 20px;
  overflow: hidden;
  background: #d3ddeb;
}

.list-row {
  display: grid;
  gap: 10px;
  padding: 16px 18px;
  background: white;
  color: inherit;
  text-decoration: none;
}

.meta {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  color: #516781;
  font-size: 0.86rem;
}

.primary,
.secondary {
  width: fit-content;
  border-radius: 999px;
  padding: 11px 18px;
  font: inherit;
}

.primary {
  border: 0;
  background: #10203a;
  color: white;
}

.secondary {
  border: 1px solid #ccd8e6;
  background: white;
  color: #10203a;
}

@media (min-width: 860px) {
  .toolbar {
    grid-template-columns: minmax(0, 1fr) 180px auto;
  }
}
</style>
