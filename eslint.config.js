import js from "@eslint/js";
import stylistic from "@stylistic/eslint-plugin";
import pluginVue from "eslint-plugin-vue";
import globals from "globals";
import tseslint from "typescript-eslint";

// ESLint formats TS and Vue; Prettier only handles CSS (it cannot print stroustrup braces).
export default tseslint.config(
	{
		ignores: ["dist/**", "src-tauri/**", "target/**", "crates/**", "node_modules/**"],
	},
	js.configs.recommended,
	...tseslint.configs.recommended,
	...pluginVue.configs["flat/recommended"],
	{
		files: ["**/*.vue"],
		languageOptions: {
			parserOptions: {
				parser: tseslint.parser,
			},
		},
	},
	{
		files: ["**/*.{ts,vue,js}"],
		languageOptions: {
			globals: {
				...globals.browser,
			},
		},
		plugins: {
			"@stylistic": stylistic,
		},
		rules: {
			"curly": ["error", "all"],
			"@stylistic/indent": ["error", "tab"],
			"@stylistic/no-tabs": "off",
			"@stylistic/brace-style": ["error", "stroustrup"],
			"@stylistic/max-len": ["error", { code: 100, tabWidth: 4 }],
			"@stylistic/semi": ["error", "always"],
			"@stylistic/quotes": ["error", "double", { avoidEscape: true }],
			"@stylistic/comma-dangle": ["error", "always-multiline"],
			"@stylistic/newline-per-chained-call": ["error", { ignoreChainWithDepth: 2 }],
			"@stylistic/eol-last": ["error", "always"],
			"@stylistic/no-trailing-spaces": "error",
			"vue/html-indent": ["error", "tab"],
			"vue/script-indent": "off",
			"vue/no-v-html": "error",
			"vue/multi-word-component-names": "off",
		},
	},
	{
		files: ["scripts/**/*.js"],
		languageOptions: {
			globals: {
				...globals.node,
			},
		},
	},
);
