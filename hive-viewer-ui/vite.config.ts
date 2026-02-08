import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      '/ws': {
        target: 'ws://127.0.0.1:9090',
        ws: true,
      },
      '/health': {
        target: 'http://127.0.0.1:9090',
      },
    },
  },
  build: {
    outDir: 'dist',
  },
})
