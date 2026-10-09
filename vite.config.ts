import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'

// Tauri 需要固定的端口与严格模式（见 ai-docs/03-build-and-sidecar.md § 3）
export default defineConfig({
  plugins: [vue(), UnoCSS()],
  // Vite 的输出目录：tauri.conf.json 的 `frontendDist` 指向它
  build: {
    // antd + xterm 本来就大，而桌面端是从本地磁盘加载的 —— 这条阈值对我们没意义，
    // 调高它只是为了让构建日志干净（不是「优化掉了体积」）。
    chunkSizeWarningLimit: 1500,
    outDir: 'dist',
    // 桌面 WebView 都是新版，不必为老浏览器降级
    target: 'esnext',
    sourcemap: false,
  },
  // 开发时别在 Tauri 的窗口里刷屏（Vite 7 里它是顶层选项）
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  // 环境变量以 TAURI_ 开头才暴露给前端（默认严格）
  envPrefix: ['VITE_', 'TAURI_'],
})
