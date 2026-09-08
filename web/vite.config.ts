import { fileURLToPath, URL } from "node:url";

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// The build lands inside the Python package, so a wheel carries the UI and
// a deployed deck needs no Node at all. Node is a build-time tool here,
// never a runtime dependency on a machine that holds exchange keys.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // The `@/` alias is declared in tsconfig.json for the editor and the
  // type-checker; the bundler needs to be told separately, and a build
  // that resolves differently from the type-check is a class of bug
  // worth spending four lines to rule out.
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  build: {
    outDir: "../src/oq_deck/web",
    emptyOutDir: true,
  },
  server: {
    port: 5173,
    proxy: { "/api": "http://127.0.0.1:8899" },
  },
});
