/// <reference types="vitest/config" />
import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { VitePWA } from "vite-plugin-pwa";

// The Saldo server the dev server forwards `/api` to (`npm run dev`).
const apiTarget = process.env.SALDO_DEV_API ?? "http://127.0.0.1:8080";

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    // The service worker caches only the app shell (HTML, JS, CSS, icons).
    // API responses are never cached: Saldo has no offline data.
    VitePWA({
      registerType: "autoUpdate",
      includeAssets: ["favicon.svg", "apple-touch-icon.png"],
      manifest: {
        name: "Saldo",
        short_name: "Saldo",
        description: "Self-hosted personal finance",
        theme_color: "#0f766e",
        background_color: "#ffffff",
        display: "standalone",
        start_url: "/",
        scope: "/",
        icons: [
          { src: "pwa-192.png", sizes: "192x192", type: "image/png" },
          { src: "pwa-512.png", sizes: "512x512", type: "image/png" },
          { src: "pwa-maskable-512.png", sizes: "512x512", type: "image/png", purpose: "maskable" },
        ],
      },
      workbox: {
        globPatterns: ["**/*.{js,css,html,svg,png,woff2}"],
        navigateFallback: "/index.html",
        navigateFallbackDenylist: [/^\/api\//, /^\/healthz/],
      },
    }),
  ],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  // `dist/.vite/manifest.json` lets `npm run size` measure the initial route.
  build: { manifest: true },
  server: {
    proxy: {
      // Same-origin in development too, so the session cookie and the CSRF
      // origin check work exactly as in production.
      "/api": { target: apiTarget, changeOrigin: false },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    css: false,
    // jsdom's fetch needs absolute URLs; the app uses same-origin paths.
    env: { VITE_SALDO_API_URL: "http://saldo.test" },
    coverage: {
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/api/schema.d.ts", "src/**/*.test.{ts,tsx}", "src/test/**"],
    },
  },
});
