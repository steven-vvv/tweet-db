import type { RouteRecordRaw } from 'vue-router'

import MobileHomePage from './pages/MobileHomePage.vue'

export const mobileBrowseRoutes: RouteRecordRaw[] = [
  { path: '/', redirect: '/mobile/browse' },
  { path: '/mobile/browse', component: MobileHomePage },
]
