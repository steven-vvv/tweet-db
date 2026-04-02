import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'

import App from './App.vue'
import { fetchInternalSession } from './api'
import { routes } from './routes'

const router = createRouter({
  history: createWebHistory(),
  routes,
})

router.beforeEach(async (to) => {
  if (!to.meta.requiresAdmin) {
    return true
  }

  try {
    const session = await fetchInternalSession()
    if (!session.authenticated || session.disabled) {
      return '/account'
    }
    if (!session.is_admin) {
      return '/forbidden'
    }
  } catch {
    return '/account'
  }

  return true
})

createApp(App).use(router).mount('#app')
