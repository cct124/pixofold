import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**', '**/.tools/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    // 保持资源为同源文件，兼容桌面端不允许 data: 图片的 CSP。
    assetsInlineLimit: 0,
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari14',
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
  },
});
