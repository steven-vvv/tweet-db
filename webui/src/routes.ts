import type { RouteRecordRaw } from 'vue-router'

import AccountPage from './components/AccountPage.vue'

export const routes: RouteRecordRaw[] = [
  { path: '/', redirect: '/account' },
  { path: '/login', redirect: '/account' },
  { path: '/register', redirect: '/account' },
  { path: '/account', component: AccountPage },
]
