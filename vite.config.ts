import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [react()],
  // Keep Rust compiler errors visible in the `pnpm tauri dev` terminal.
  clearScreen: false,
  server: {
    // Tauri's `devUrl` points here, so the port must not drift.
    port: 1420,
    strictPort: true,
    watch: {
      // Cargo rebuilds the backend itself; Vite reloading on target/ churn is noise.
      ignored: ['**/src-tauri/**'],
    },
  },
  test: {
    environment: 'jsdom',
    // Jest-style globals, so component tests written for Jest run unchanged.
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
  },
});
