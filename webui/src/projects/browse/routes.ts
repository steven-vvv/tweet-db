import type { RouteRecordRaw } from 'vue-router'

import BrowseTimelinePage from './pages/BrowseTimelinePage.vue'
import BrowseTweetDetailPage from './pages/BrowseTweetDetailPage.vue'
import BrowseUserDetailPage from './pages/BrowseUserDetailPage.vue'

export const browseRoutes: RouteRecordRaw[] = [
  { path: '/', redirect: '/browse' },
  { path: '/browse', component: BrowseTimelinePage },
  { path: '/browse/tweets/:tweetId', component: BrowseTweetDetailPage },
  { path: '/browse/users/:userId', component: BrowseUserDetailPage },
]
