<template>
  <section class="admin-page">
    <header class="admin-head">
      <div>
        <h1>Accounts</h1>
        <p>Local users, roles, and account availability.</p>
      </div>
    </header>

    <form class="toolbar" @submit.prevent="reload">
      <input v-model="q" type="search" placeholder="Username or user ID prefix" />
      <select v-model="status">
        <option value="all">All statuses</option>
        <option value="active">Active</option>
        <option value="pending">Pending activation</option>
        <option value="disabled">Disabled</option>
      </select>
      <select v-model="role">
        <option value="all">All roles</option>
        <option value="admin">Admin</option>
        <option value="user">User</option>
      </select>
      <button type="submit" class="primary" :disabled="loading">Search</button>
    </form>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="table">
      <div class="table-head cols">
        <span>Username</span>
        <span>Status</span>
        <span>Role</span>
        <span>Created</span>
      </div>
      <RouterLink
        v-for="item in items"
        :key="item.id"
        :to="`/admin/users/${item.id}`"
        class="table-row cols"
      >
        <strong>{{ item.username }}</strong>
        <span class="badge" :class="statusTone(accountStatus(item))">
          {{ accountStatusLabel(item) }}
        </span>
        <span>{{ item.isAdmin ? 'Admin' : 'User' }}</span>
        <span class="muted">{{ item.createdAt }}</span>
      </RouterLink>
      <p v-if="!loading && items.length === 0" class="empty">No accounts matched.</p>
    </section>

    <button v-if="nextCursor" type="button" :disabled="loading" @click="loadMore">
      Load more
    </button>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'

import { accountStatus, accountStatusLabel, statusTone } from '../../shared/admin-helpers'
import { fetchAdminUsers, type JsonRecord } from '../../shared/api'

const items = ref<JsonRecord[]>([])
const nextCursor = ref<string | null>(null)
const q = ref('')
const status = ref('all')
const role = ref('all')
const loading = ref(false)
const error = ref('')

onMounted(reload)

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
      role: role.value,
      cursor: reset ? undefined : nextCursor.value,
    })
    items.value = reset ? response.items : [...items.value, ...response.items]
    nextCursor.value = response.nextCursor
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load accounts'
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.cols {
  grid-template-columns: minmax(180px, 1.5fr) 110px 90px 220px;
}

@media (max-width: 860px) {
  .cols {
    grid-template-columns: 1fr;
  }
}
</style>
