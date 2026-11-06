import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Tauri expects a fixed port and must see Rust errors in the terminal.
export default defineConfig({
	plugins: [vue()],
	clearScreen: false,
	server: {
		port: 1420,
		strictPort: true,
		watch: {
			ignored: ["**/src-tauri/**", "**/target/**"],
		},
	},
	build: {
		target: "es2022",
	},
	test: {
		environment: "jsdom",
	},
});
