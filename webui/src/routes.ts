import type { RouteRecordRaw } from 'vue-router'

import AccountPage from './components/AccountPage.vue'
import ForbiddenPage from './components/ForbiddenPage.vue'
import ActorDetailPage from './components/admin/ActorDetailPage.vue'
import ActorsPage from './components/admin/ActorsPage.vue'
import AdminShell from './components/admin/AdminShell.vue'
import MediaDetailPage from './components/admin/MediaDetailPage.vue'
import PostDetailPage from './components/admin/PostDetailPage.vue'
import PostsPage from './components/admin/PostsPage.vue'
import StorageObjectDetailPage from './components/admin/StorageObjectDetailPage.vue'
import StorageObjectsPage from './components/admin/StorageObjectsPage.vue'
import TransferJobDetailPage from './components/admin/TransferJobDetailPage.vue'
import TransfersPage from './components/admin/TransfersPage.vue'
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
      { path: '', redirect: '/admin/users' },
      { path: 'users', component: UsersPage },
      { path: 'users/:userId', component: UserDetailPage },
      { path: 'posts', component: PostsPage },
      { path: 'posts/:sourceKind/:sourcePostId', component: PostDetailPage },
      { path: 'actors', component: ActorsPage },
      { path: 'actors/:sourceKind/:sourceActorId', component: ActorDetailPage },
      { path: 'media/:mediaId', component: MediaDetailPage },
      { path: 'storage-objects', component: StorageObjectsPage },
      { path: 'storage-objects/:objectId', component: StorageObjectDetailPage },
      { path: 'transfers', component: TransfersPage },
      { path: 'transfers/jobs/:jobId', component: TransferJobDetailPage },
    ],
  },
]
