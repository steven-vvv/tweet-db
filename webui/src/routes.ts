import type { RouteRecordRaw } from 'vue-router'

import AccountPage from './components/AccountPage.vue'
import ForbiddenPage from './components/ForbiddenPage.vue'
import AdminShell from './components/admin/AdminShell.vue'
import MediaDetailPage from './components/admin/MediaDetailPage.vue'
import MediaPage from './components/admin/MediaPage.vue'
import OverviewPage from './components/admin/OverviewPage.vue'
import StorageObjectDetailPage from './components/admin/StorageObjectDetailPage.vue'
import StorageObjectsPage from './components/admin/StorageObjectsPage.vue'
import TransferTaskDetailPage from './components/admin/TransferTaskDetailPage.vue'
import TransfersPage from './components/admin/TransfersPage.vue'
import TweetDetailPage from './components/admin/TweetDetailPage.vue'
import TweetsPage from './components/admin/TweetsPage.vue'
import TwitterUserDetailPage from './components/admin/TwitterUserDetailPage.vue'
import TwitterUsersPage from './components/admin/TwitterUsersPage.vue'
import UserDetailPage from './components/admin/UserDetailPage.vue'
import UsersPage from './components/admin/UsersPage.vue'

export const routes: RouteRecordRaw[] = [
  { path: '/', redirect: '/account' },
  { path: '/login', redirect: '/account' },
  { path: '/register', redirect: '/account' },
  { path: '/account', component: AccountPage, meta: { wide: false } },
  { path: '/forbidden', component: ForbiddenPage, meta: { wide: false } },
  {
    path: '/admin',
    component: AdminShell,
    meta: { wide: true, requiresAdmin: true },
    children: [
      { path: '', redirect: '/admin/overview' },
      { path: 'overview', component: OverviewPage },
      { path: 'users', component: UsersPage },
      { path: 'users/:userId', component: UserDetailPage },
      { path: 'twitter-users', component: TwitterUsersPage },
      { path: 'twitter-users/:userId', component: TwitterUserDetailPage },
      { path: 'tweets', component: TweetsPage },
      { path: 'tweets/:tweetId', component: TweetDetailPage },
      { path: 'media', component: MediaPage },
      { path: 'media/:mediaId', component: MediaDetailPage },
      { path: 'transfers', component: TransfersPage },
      { path: 'transfers/tasks/:taskId', component: TransferTaskDetailPage },
      { path: 'storage-objects', component: StorageObjectsPage },
      { path: 'storage-objects/:objectId', component: StorageObjectDetailPage },
    ],
  },
]
