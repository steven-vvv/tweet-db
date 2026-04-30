import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'

import { fetchInternalSession } from '../../shared/api'
import BrowseApp from './BrowseApp.vue'
import { browseRoutes } from './routes'

const router = createRouter({
  history: createWebHistory(),
  routes: browseRoutes,
})

router.beforeEach(async () => {
  try {
    const session = await fetchInternalSession()
    if (!session.authenticated || session.disabled) {
      window.location.href = '/account'
      return false
    }
    if (!session.is_admin) {
      window.location.href = '/forbidden'
      return false
    }
  } catch {
    window.location.href = '/account'
    return false
  }

  return true
})

createApp(BrowseApp).use(router).mount('#app')
