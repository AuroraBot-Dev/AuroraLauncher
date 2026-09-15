import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import { useAppStore } from './stores/app'
import './style.css'

const app = createApp(App)
const pinia = createPinia()

app.use(pinia)
app.use(router)

// 唯一的初始化入口：先注册事件监听并确定运行模式，再挂载。
// 这样 App.vue 挂载时 mode 已就绪，启动后的自动检查更新才能正常安排。
const appStore = useAppStore()
appStore.initListeners()
appStore.refreshAll()

app.mount('#app')
