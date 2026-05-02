import type { RouteRecordRaw } from 'vue-router'

import MobileHomePage from './pages/MobileHomePage.vue'
import MobileSearchPage from './pages/MobileSearchPage.vue'

export const mobileBrowseRoutes: RouteRecordRaw[] = [
  { path: '/', redirect: '/mobile/browse' },
  { path: '/mobile/browse', component: MobileHomePage },
  { path: '/mobile/browse/search', component: MobileSearchPage },
]
