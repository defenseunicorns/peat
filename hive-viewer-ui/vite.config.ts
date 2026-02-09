import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 3001,
    host: '0.0.0.0',
    allowedHosts: ['kitlab'],
    proxy: {
      '/ws': {
        target: 'ws://127.0.0.1:9090',
        ws: true,
        changeOrigin: true,
      },
      '/health': {
        target: 'http://127.0.0.1:9090',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: 'dist',
  },
})
