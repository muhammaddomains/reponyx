import tseslint from "typescript-eslint";
import createBaseConfig from "@reponyx/config/eslint/base.mjs";

export default [
  { ignores: ["dist/**", "node_modules/**", "src-tauri/**"] },
  ...createBaseConfig(tseslint),
];
