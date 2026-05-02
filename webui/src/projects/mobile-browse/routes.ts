import type { RouteRecordRaw } from 'vue-router'

import MobileHomePage from './pages/MobileHomePage.vue'
import MobileSearchPage from './pages/MobileSearchPage.vue'
import MobileTweetDetailPage from './pages/MobileTweetDetailPage.vue'
import MobileUserDetailPage from './pages/MobileUserDetailPage.vue'

export const mobileBrowseRoutes: RouteRecordRaw[] = [
  { path: '/', redirect: '/mobile/browse' },
  { path: '/mobile/browse', component: MobileHomePage },
  { path: '/mobile/browse/search', component: MobileSearchPage },
  { path: '/mobile/browse/tweets/:tweetId', component: MobileTweetDetailPage },
  { path: '/mobile/browse/users/:userId', component: MobileUserDetailPage },
]
