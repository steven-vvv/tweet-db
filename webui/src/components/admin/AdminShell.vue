<template>
  <section class="admin-shell">
    <aside class="sidebar">
      <RouterLink to="/admin" class="brand">
        <strong>tweet-db</strong>
        <span>Admin</span>
      </RouterLink>

      <nav class="nav">
        <RouterLink v-for="item in navItems" :key="item.to" :to="item.to" class="nav-link">
          {{ item.label }}
        </RouterLink>
      </nav>

      <div class="session">
        <span>{{ session?.username ?? 'admin' }}</span>
        <button type="button" @click="goAccount">Account</button>
      </div>
    </aside>

    <section class="content">
      <RouterView />
    </section>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { RouterLink, RouterView, useRouter } from 'vue-router'

import { fetchInternalSession, type InternalSessionResponse } from '../../api'

const router = useRouter()
const session = ref<InternalSessionResponse | null>(null)

const navItems = [
  { to: '/admin/overview', label: 'Overview' },
  { to: '/admin/users', label: 'Accounts' },
  { to: '/admin/twitter-users', label: 'X Users' },
  { to: '/admin/tweets', label: 'Tweets' },
  { to: '/admin/media', label: 'Media' },
  { to: '/admin/transfers', label: 'Transfers' },
  { to: '/admin/storage-objects', label: 'Storage' },
]

onMounted(async () => {
  try {
    session.value = await fetchInternalSession()
  } catch {
    session.value = null
  }
})

function goAccount() {
  router.push('/account')
}
</script>

<style scoped>
.admin-shell {
  min-height: 100vh;
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
  color: #172033;
}

.sidebar {
  position: sticky;
  top: 0;
  height: 100vh;
  display: grid;
  grid-template-rows: auto 1fr auto;
  gap: 20px;
  padding: 18px 14px;
  border-right: 1px solid #d9e0ea;
  background: #fff;
}

.brand {
  display: grid;
  gap: 2px;
  color: inherit;
  text-decoration: none;
}

.brand strong {
  font-size: 1rem;
}

.brand span,
.session span {
  color: #65748b;
  font-size: 0.82rem;
}

.nav {
  display: grid;
  align-content: start;
  gap: 4px;
}

.nav-link {
  padding: 9px 10px;
  border-radius: 7px;
  color: #34445c;
  text-decoration: none;
  font-size: 0.94rem;
}

.nav-link.router-link-active {
  background: #eaf0f7;
  color: #101827;
  font-weight: 650;
}

.session {
  display: grid;
  gap: 8px;
}

.session button {
  width: fit-content;
  border: 1px solid #cfd8e3;
  border-radius: 6px;
  background: #fff;
  color: #172033;
  padding: 7px 10px;
  font: inherit;
}

.content {
  min-width: 0;
  padding: 22px;
}

:global(.admin-page) {
  display: grid;
  gap: 14px;
}

:global(.admin-head) {
  display: flex;
  align-items: end;
  justify-content: space-between;
  gap: 16px;
}

:global(.admin-head h1) {
  margin: 0;
  font-size: 1.35rem;
  line-height: 1.2;
}

:global(.admin-head p) {
  margin: 4px 0 0;
  color: #627087;
}

:global(.toolbar) {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  padding: 10px;
  border: 1px solid #d9e0ea;
  border-radius: 8px;
  background: #fff;
}

:global(.toolbar input),
:global(.toolbar select) {
  min-width: 180px;
  border: 1px solid #cbd5e1;
  border-radius: 6px;
  padding: 8px 10px;
  background: #fff;
  color: #172033;
  font: inherit;
}

:global(.toolbar input[type='search']) {
  flex: 1 1 260px;
}

:global(button),
:global(.button-link) {
  border: 1px solid #cbd5e1;
  border-radius: 6px;
  background: #fff;
  color: #172033;
  padding: 8px 11px;
  font: inherit;
  text-decoration: none;
  cursor: pointer;
}

:global(button.primary),
:global(.button-link.primary) {
  border-color: #172033;
  background: #172033;
  color: #fff;
}

:global(button.danger) {
  border-color: #b42318;
  color: #b42318;
}

:global(button:disabled) {
  cursor: default;
  opacity: 0.55;
}

:global(.panel) {
  border: 1px solid #d9e0ea;
  border-radius: 8px;
  background: #fff;
}

:global(.panel-pad) {
  padding: 14px;
}

:global(.grid-cards) {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: 10px;
}

:global(.stat) {
  display: grid;
  gap: 5px;
  padding: 13px;
  border: 1px solid #d9e0ea;
  border-radius: 8px;
  background: #fff;
}

:global(.stat span) {
  color: #66748a;
  font-size: 0.78rem;
}

:global(.stat strong) {
  font-size: 1.35rem;
}

:global(.table) {
  display: grid;
  border: 1px solid #d9e0ea;
  border-radius: 8px;
  overflow: hidden;
  background: #fff;
}

:global(.table-head),
:global(.table-row) {
  display: grid;
  gap: 10px;
  align-items: center;
  padding: 10px 12px;
}

:global(.table-head) {
  background: #f1f5f9;
  color: #627087;
  font-size: 0.78rem;
  font-weight: 650;
  text-transform: uppercase;
}

:global(.table-row) {
  min-height: 46px;
  border-top: 1px solid #edf1f6;
  color: inherit;
  text-decoration: none;
}

:global(.table-row:hover) {
  background: #f8fafc;
}

:global(.muted) {
  color: #66748a;
}

:global(.mono) {
  font-family: "IBM Plex Mono", ui-monospace, monospace;
  font-size: 0.82rem;
}

:global(.error) {
  margin: 0;
  padding: 10px 12px;
  border: 1px solid #f1b8b8;
  border-radius: 8px;
  background: #fff7f7;
  color: #a12231;
}

:global(.empty) {
  margin: 0;
  padding: 16px;
  color: #66748a;
}

:global(.badge) {
  width: fit-content;
  border-radius: 999px;
  padding: 3px 8px;
  background: #eef2f7;
  color: #475569;
  font-size: 0.78rem;
  font-weight: 650;
}

:global(.badge.good) {
  background: #e8f5ee;
  color: #17663a;
}

:global(.badge.warn) {
  background: #fff4d8;
  color: #7a4b00;
}

:global(.badge.bad) {
  background: #ffe8e6;
  color: #a12231;
}

:global(.kv) {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: 10px;
}

:global(.kv div) {
  display: grid;
  gap: 4px;
  padding: 10px;
  border: 1px solid #e2e8f0;
  border-radius: 6px;
}

:global(.kv span) {
  color: #66748a;
  font-size: 0.78rem;
}

:global(.kv strong) {
  min-width: 0;
  overflow-wrap: anywhere;
}

@media (max-width: 780px) {
  .admin-shell {
    grid-template-columns: 1fr;
  }

  .sidebar {
    position: static;
    height: auto;
    grid-template-rows: auto auto auto;
  }

  .nav {
    display: flex;
    flex-wrap: wrap;
  }

  .content {
    padding: 14px;
  }
}
</style>
