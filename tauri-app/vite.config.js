import { defineConfig, build as viteBuild } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { sveltekit } from "@sveltejs/kit/vite";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

/** Standalone HTML pages used by secondary Tauri windows (not SvelteKit routes). */
const standalonePages = ["overlay", "settings", "history"];

/**
 * Vite plugin that serves standalone HTML entry points (overlay, settings, history)
 * alongside the SvelteKit app. These are separate Tauri windows that don't use
 * SvelteKit routing — they mount Svelte components directly.
 *
 * - Dev: middleware intercepts requests and transforms the HTML through Vite.
 * - Build: after the SvelteKit build finishes, a secondary Vite build bundles
 *   each standalone page into the same output directory (../build).
 */
function tauriMultiWindow() {
  return {
    name: "tauri-multi-window",

    /** @param {import('vite').ViteDevServer} server */
    configureServer(server) {
      const paths = standalonePages.map((p) => `/${p}.html`);

      server.middlewares.use((req, res, next) => {
        if (paths.includes(req.url)) {
          let html = readFileSync(resolve("src" + req.url), "utf-8");
          // Rewrite relative script/link paths to /src/ so Vite can resolve them
          html = html.replace(/src="\.\/([^"]+)"/g, 'src="/src/$1"');
          html = html.replace(/href="\.\/([^"]+)"/g, 'href="/src/$1"');
          server.transformIndexHtml(req.url, html).then((transformed) => {
            res.setHeader("Content-Type", "text/html");
            res.end(transformed);
          });
          return;
        }
        next();
      });
    },

    /** After the SvelteKit build, run a second Vite build for standalone pages. */
    async closeBundle() {
      const input = Object.fromEntries(
        standalonePages.map((p) => [p, resolve("src", `${p}.html`)])
      );

      console.log(
        "\n[tauri-multi-window] Building standalone pages:",
        Object.keys(input).join(", ")
      );

      await viteBuild({
        // Use src/ as root so HTML entry paths resolve correctly and
        // output filenames don't include the "src/" prefix.
        root: resolve("src"),
        plugins: [svelte()],
        build: {
          rollupOptions: { input },
          outDir: resolve("build"),
          // Don't wipe the SvelteKit output that was already written.
          emptyOutDir: false,
        },
        // Prevent infinite recursion — this nested build must not re-run
        // the sveltekit() or tauriMultiWindow() plugins.
        configFile: false,
      });

      console.log("[tauri-multi-window] Standalone pages built successfully.\n");
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
