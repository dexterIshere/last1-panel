import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// Port fixe attendu par Tauri (tauri.conf.json, devUrl).
export default defineConfig(() => ({
  plugins: [sveltekit()],
  clearScreen: false,
  server: {
    port: 1440,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host ? { protocol: "ws", host, port: 1441 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/core/**", "**/target/**"] },
  },
}));
