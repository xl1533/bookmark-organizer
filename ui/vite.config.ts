import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Tauri 前端构建配置：
// - 开发服务器固定 5173 端口（与 src-tauri/tauri.conf.json 的 devUrl 对应）
// - 环境变量注入避免 Vite 在 Tauri WebView 中探测 Node 特性
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2021",
    outDir: "dist",
    minify: "esbuild",
    sourcemap: false,
  },
});
