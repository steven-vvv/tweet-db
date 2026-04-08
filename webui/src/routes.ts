import type { RouteRecordRaw } from 'vue-router'

import AccountPage from './components/AccountPage.vue'
import ForbiddenPage from './components/ForbiddenPage.vue'
import AdminShell from './components/admin/AdminShell.vue'
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
    ],
  },
]
