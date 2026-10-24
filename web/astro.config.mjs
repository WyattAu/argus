import { defineConfig } from "astro/config";
import solid from "@astrojs/solid-js";

// Proxy /api (including the WebSocket upgrade) to the argus core.
// Astro passes `vite.server.*` straight through to the Vite dev server.
export default defineConfig({
  integrations: [solid()],
  vite: {
    server: {
      proxy: {
        "/api": {
          target: "http://127.0.0.1:8420",
          ws: true,
          changeOrigin: true,
        },
      },
    },
  },
});
