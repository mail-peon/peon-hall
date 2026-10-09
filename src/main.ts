import { createApp } from 'vue'

import 'virtual:uno.css'
// ⚠️ 用 tailwind-**compat** 而不是 tailwind：后者的 preflight 里有
// `button, [type='button'], … { background-color: transparent }`，
// 与 antd 的 `.ant-btn` 同权重、位置更靠后 —— 结果所有普通按钮变成透明底。
// compat 版是 UnoCSS 官方为组件库准备的那份（禁用了这条与其它几条），
// 依据：https://github.com/unocss/unocss/issues/2127
import '@unocss/reset/tailwind-compat.css'
// antd 的基线样式先加载，自家样式在后（同权重时后者覆盖）
import 'ant-design-vue/dist/reset.css'
import './styles/main.css'

import App from './App.vue'

createApp(App).mount('#app')
