import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: true,
    watch: {
      // 忽略 Rust 侧目录：否则 cargo 编译时锁住 target 里的 exe，
      // Vite 的文件监听会因 EBUSY 崩溃，连带 tauri dev 一起退出。
      ignored: ["**/src-tauri/**"]
    }
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    target: "es2020"
  }
});
