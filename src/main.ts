import { createApp } from 'vue'

import 'virtual:uno.css'
import '@unocss/reset/tailwind.css'
// antd 的基线样式先加载，自家样式在后（同权重时后者覆盖）
import 'ant-design-vue/dist/reset.css'
import './styles/main.css'

import App from './App.vue'

createApp(App).mount('#app')
