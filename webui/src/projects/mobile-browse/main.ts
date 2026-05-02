import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'

import { fetchV2Me } from '../../shared/api'
import MobileBrowseApp from './MobileBrowseApp.vue'
import { mobileBrowseRoutes } from './routes'

const router = createRouter({
  history: createWebHistory(),
  routes: mobileBrowseRoutes,
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

createApp(MobileBrowseApp).use(router).mount('#app')
