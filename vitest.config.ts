// Vitest 配置：纯 TS 逻辑用 jsdom（refs + 生命周期可跑），环境变量沿用 vite.config.ts。
// 见 ai-docs/00-overview.md § 5 —— 测试只跑纯函数，不引入 vue-test-utils 的 mount（避免
// 引入 antd/xterm 的副作用，让单测保持"输入 → 输出"的形式）。

import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  test: {
    environment: 'jsdom',
    include: ['src/**/*.{test,spec}.ts'],
    // 不监听（CI 跑单次；开发者按需 `pnpm vitest`）
    watch: false,
  },
})