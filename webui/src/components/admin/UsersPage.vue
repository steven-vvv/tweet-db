<template>
  <section class="stack">
    <header class="page-head">
      <div>
        <p class="eyebrow">User Management</p>
        <h2>Accounts</h2>
      </div>
      <p class="hint">Manage account availability. Administrator assignment remains database-only.</p>
    </header>

    <form class="toolbar" @submit.prevent="reload">
      <input v-model="q" type="search" placeholder="Search username or user ID" />
      <select v-model="status">
        <option value="all">All statuses</option>
        <option value="active">Active</option>
        <option value="disabled">Disabled</option>
      </select>
      <button type="submit" class="primary" :disabled="loading">Search</button>
    </form>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="table-card">
      <div class="table-head">
        <span>Username</span>
        <span>Status</span>
        <span>Role</span>
        <span>Created</span>
      </div>
      <RouterLink
        v-for="item in items"
        :key="item.id"
        :to="`/admin/users/${item.id}`"
        class="table-row"
      >
        <strong>{{ item.username }}</strong>
        <span>{{ item.disabled ? 'Disabled' : 'Active' }}</span>
        <span>{{ item.isAdmin ? 'Admin' : 'User' }}</span>
        <span>{{ item.createdAt }}</span>
      </RouterLink>
      <p v-if="!loading && items.length === 0" class="empty">No accounts matched the current filter.</p>
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

import { fetchAdminUsers, type JsonRecord } from '../../api'

const items = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const q = ref('')
const status = ref('all')
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
    const response = await fetchAdminUsers({
      q: q.value.trim() || undefined,
      status: status.value,
      cursor: reset ? undefined : nextCursor.value,
    })
    items.value = reset ? response.items : [...items.value, ...response.items]
    nextCursor.value = response.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load users'
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
.empty,
.error {
  margin: 0;
}

.hint,
.empty {
  color: #516781;
}

.error {
  color: #a12231;
}

.toolbar {
  display: grid;
  gap: 12px;
}

.toolbar input,
.toolbar select {
  width: 100%;
  border: 1px solid #cbd6e2;
  border-radius: 14px;
  padding: 12px 14px;
  font: inherit;
}

.table-card {
  display: grid;
  border: 1px solid #d3ddeb;
  border-radius: 20px;
  overflow: hidden;
  background: rgba(255, 255, 255, 0.88);
}

.table-head,
.table-row {
  display: grid;
  gap: 12px;
  padding: 14px 18px;
}

.table-head {
  background: #eef3f8;
  color: #516781;
  font-size: 0.82rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.table-row {
  color: inherit;
  text-decoration: none;
  border-top: 1px solid #edf2f7;
}

.table-row strong {
  font-size: 1rem;
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

  .table-head,
  .table-row {
    grid-template-columns: minmax(0, 1.4fr) 140px 120px 220px;
    align-items: center;
  }
}
</style>
