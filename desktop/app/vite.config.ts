import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The UI lives in ui/; Tauri loads the built files from dist/.
export default defineConfig({
  root: "ui",
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { outDir: "../dist", emptyOutDir: true },
});
