import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'node:path'

export default defineConfig({
  plugins: [vue()],
  server: {
    port: 5173
  },
  build: {
    rollupOptions: {
      input: {
        root: resolve(__dirname, 'index.html'),
        account: resolve(__dirname, 'account/index.html'),
        admin: resolve(__dirname, 'admin/index.html'),
        browse: resolve(__dirname, 'browse/index.html'),
      },
    },
  }
})
