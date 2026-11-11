import { defineConfig } from "vite";
export default defineConfig({
  server: {
    proxy: {
      "/management": "http://127.0.0.1:21114",
      "/api": "http://127.0.0.1:21114",
    },
  },
  build: { sourcemap: false },
});
