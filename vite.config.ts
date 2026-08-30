import { defineConfig } from 'vite';

export default defineConfig({
  publicDir: 'model_material',
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: 'chrome105',
    minify: 'oxc',
    sourcemap: false,
    rolldownOptions: {
      input: ['index.html', 'settings.html'],
    },
  },
});
