/// <reference types="vite/client" />

// UnoCSS 的虚拟模块：Vite 插件在运行期提供它，但 TS 不知道它存在
declare module 'virtual:uno.css' {
  const css: string
  export default css
}

// `.vue` 单文件组件（vue-tsc 自己认识，但普通的 tsc 编辑器插件需要）
declare module '*.vue' {
  import type { DefineComponent } from 'vue'

  const component: DefineComponent<Record<string, never>, Record<string, never>, unknown>
  export default component
}
