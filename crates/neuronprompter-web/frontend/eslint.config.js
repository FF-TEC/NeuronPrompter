// ESLint configuration using flat config format (ESLint 9+).
// The @typescript-eslint packages used here are the legacy separate packages
// (@typescript-eslint/eslint-plugin and @typescript-eslint/parser).
// A future migration to the unified 'typescript-eslint' package would simplify
// the configuration and align with the recommended setup for ESLint 9+.

import tseslint from "@typescript-eslint/eslint-plugin";
import tsparser from "@typescript-eslint/parser";

export default [
  {
    files: ["src/**/*.ts", "src/**/*.tsx"],
    languageOptions: {
      parser: tsparser,
      parserOptions: {
        ecmaVersion: "latest",
        sourceType: "module",
        jsxPragma: undefined, // SolidJS uses JSX transform, not React pragma
      },
    },
    plugins: {
      "@typescript-eslint": tseslint,
    },
    rules: {
      // typescript-eslint recommended rules (subset)
      "@typescript-eslint/no-unused-vars": ["warn", { argsIgnorePattern: "^_" }],
      "@typescript-eslint/no-explicit-any": "warn",
      "@typescript-eslint/no-non-null-assertion": "off", // Used intentionally after length checks
      "@typescript-eslint/consistent-type-imports": ["warn", { prefer: "type-imports" }],

      // General quality
      "no-console": ["warn", { allow: ["warn", "error"] }],
      "no-debugger": "error",
      "prefer-const": "warn",
      eqeqeq: ["error", "always"],
    },
  },
  {
    ignores: ["dist/**", "node_modules/**"],
  },
];
