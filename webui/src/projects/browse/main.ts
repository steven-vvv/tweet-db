import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'

import { fetchV2Me } from '../../shared/api'
import BrowseApp from './BrowseApp.vue'
import { browseRoutes } from './routes'

const router = createRouter({
  history: createWebHistory(),
  routes: browseRoutes,
})

router.beforeEach(async () => {
  try {
    const response = await fetchV2Me({ include: 'capabilities' })
    if (!response.data.isAdmin) {
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
