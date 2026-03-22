import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

/**
 * Vite plugin that serves standalone HTML entry points (overlay, settings, history)
 * alongside the SvelteKit app. These are separate Tauri windows that don't use
 * SvelteKit routing — they mount Svelte components directly.
 */
function tauriMultiWindow() {
  return {
    name: "tauri-multi-window",
    /** @param {import('vite').ViteDevServer} server */
    configureServer(server) {
      // In dev mode, serve standalone HTML pages via Vite's transform pipeline
      const standalonePages = ["/overlay.html", "/settings.html", "/history.html"];

      server.middlewares.use((req, res, next) => {
        if (standalonePages.includes(req.url)) {
          const html = readFileSync(resolve("src" + req.url), "utf-8");
          server.transformIndexHtml(req.url, html).then((transformed) => {
            res.setHeader("Content-Type", "text/html");
            res.end(transformed);
          });
          return;
        }
        next();
      });
    },
  };
}

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [sveltekit(), tauriMultiWindow()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
