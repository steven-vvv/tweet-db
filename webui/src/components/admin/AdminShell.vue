<template>
  <section class="admin-shell">
    <aside class="sidebar">
      <div class="sidebar-head">
        <p class="eyebrow">Admin Console</p>
        <h2>tweet-db operations</h2>
      </div>
      <nav class="nav">
        <RouterLink v-for="item in navItems" :key="item.to" :to="item.to" class="nav-link">
          <span>{{ item.label }}</span>
          <small>{{ item.description }}</small>
        </RouterLink>
      </nav>
      <div class="session-card">
        <p class="session-label">Current session</p>
        <strong>{{ session?.username ?? 'admin' }}</strong>
        <button type="button" class="secondary" @click="goAccount">Account</button>
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
  { to: '/admin/users', label: 'Users', description: 'Accounts and enable or disable actions' },
  { to: '/admin/posts', label: 'Posts', description: 'Post browse, search, and raw records' },
  { to: '/admin/actors', label: 'Actors', description: 'Actor browse, detail, and linked media' },
  {
    to: '/admin/storage-objects',
    label: 'Resources',
    description: 'Stored objects and signed download links',
  },
  {
    to: '/admin/transfers',
    label: 'Transfers',
    description: 'Transfer overview, jobs, attempts, and retries',
  },
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
  display: grid;
  gap: 24px;
}

.sidebar {
  display: grid;
  gap: 18px;
  padding: 22px;
  border: 1px solid rgba(16, 32, 58, 0.08);
  border-radius: 22px;
  background:
    linear-gradient(180deg, rgba(15, 31, 54, 0.95), rgba(15, 31, 54, 0.9)),
    linear-gradient(135deg, rgba(126, 198, 255, 0.15), transparent);
  color: white;
}

.sidebar-head h2 {
  margin: 0;
  font-size: 1.35rem;
}

.eyebrow {
  margin: 0 0 8px;
  font-size: 0.74rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: rgba(255, 255, 255, 0.72);
}

.nav {
  display: grid;
  gap: 10px;
}

.nav-link {
  display: grid;
  gap: 4px;
  padding: 12px 14px;
  border-radius: 16px;
  color: inherit;
  text-decoration: none;
  background: rgba(255, 255, 255, 0.06);
}

.nav-link.router-link-active {
  background: rgba(126, 198, 255, 0.22);
}

.nav-link small {
  color: rgba(255, 255, 255, 0.76);
}

.session-card {
  display: grid;
  gap: 8px;
  padding-top: 8px;
}

.session-label {
  margin: 0;
  color: rgba(255, 255, 255, 0.72);
}

.secondary {
  width: fit-content;
  border: 1px solid rgba(255, 255, 255, 0.22);
  border-radius: 999px;
  background: transparent;
  color: white;
  padding: 10px 16px;
  font: inherit;
}

.content {
  min-width: 0;
}

@media (min-width: 960px) {
  .admin-shell {
    grid-template-columns: 280px minmax(0, 1fr);
    align-items: start;
  }

  .sidebar {
    position: sticky;
    top: 24px;
  }
}
</style>
