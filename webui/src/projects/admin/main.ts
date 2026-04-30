import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'

import { fetchInternalSession } from '../../shared/api'
import AdminApp from './AdminApp.vue'
import { adminRoutes } from './routes'

const router = createRouter({
  history: createWebHistory(),
  routes: adminRoutes,
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

createApp(AdminApp).use(router).mount('#app')
