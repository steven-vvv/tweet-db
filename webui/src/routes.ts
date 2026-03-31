import type { RouteRecordRaw } from 'vue-router'

import LoginPage from './components/LoginPage.vue'
import RegisterPage from './components/RegisterPage.vue'
import AccountPage from './components/AccountPage.vue'

export const routes: RouteRecordRaw[] = [
  { path: '/', redirect: '/login' },
  { path: '/login', component: LoginPage },
  { path: '/register', component: RegisterPage },
  { path: '/account', component: AccountPage },
]

