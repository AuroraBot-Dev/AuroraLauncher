import { createRouter, createWebHashHistory } from 'vue-router'
import type { RouteRecordRaw } from 'vue-router'

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    name: 'dashboard',
    component: () => import('../views/Dashboard.vue')
  },
  {
    path: '/chat',
    name: 'chat',
    component: () => import('../views/Chat.vue')
  },
  {
    path: '/environment',
    name: 'environment',
    component: () => import('../views/Environment.vue')
  },
  {
    path: '/kernel',
    name: 'kernel',
    component: () => import('../views/Kernel.vue')
  },
  {
    path: '/config',
    name: 'config',
    component: () => import('../views/Config.vue')
  },

  {
    path: '/logs',
    name: 'logs',
    component: () => import('../views/Logs.vue')
  },
  {
    path: '/settings',
    name: 'settings',
    component: () => import('../views/Settings.vue')
  },
  {
    path: '/about',
    name: 'about',
    component: () => import('../views/About.vue')
  },
  {
    path: '/:pathMatch(.*)*',
    redirect: '/'
  }
]

const router = createRouter({
  history: createWebHashHistory(),
  routes
})

export default router
