import { defineConfig, presetWind4 } from 'unocss'

// ADR-1001 Q5：用 UnoCSS 的原子类，非原子部分（卡片阴影、状态色变量）写在 src/styles/ 里
export default defineConfig({
  presets: [presetWind4()],
  // 深色模式跟随系统：`dark:` 变体走 prefers-color-scheme
  theme: {
    colors: {
      // 状态色（四个状态各一个）；深浅两套在 styles/main.css 里用 CSS 变量定义
      state: {
        running: 'var(--state-running)',
        stopped: 'var(--state-stopped)',
        foreground: 'var(--state-foreground)',
        none: 'var(--state-none)',
      },
    },
  },
  shortcuts: {
    // 按钮：三种优先级
    btn: 'px-3 py-1.5 rounded-md text-sm font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed',
    'btn-primary': 'btn bg-blue-600 text-white hover:bg-blue-700',
    'btn-secondary': 'btn bg-gray-200 text-gray-800 hover:bg-gray-300 dark:bg-gray-700 dark:text-gray-100 dark:hover:bg-gray-600',
    'btn-danger': 'btn bg-red-600 text-white hover:bg-red-700',
    card: 'rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4',
  },
})
