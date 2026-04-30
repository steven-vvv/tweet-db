import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'

import AccountPage from '../../components/AccountPage.vue'
import ForbiddenPage from '../../components/ForbiddenPage.vue'
import AccountApp from './AccountApp.vue'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', redirect: '/browse' },
    { path: '/login', redirect: '/account' },
    { path: '/register', redirect: '/account' },
    { path: '/account', component: AccountPage },
    { path: '/forbidden', component: ForbiddenPage },
    { path: '/account/:pathMatch(.*)*', component: AccountPage },
  ],
})

createApp(AccountApp).use(router).mount('#app')
