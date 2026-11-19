// Shared ESLint flat config for RepoNyx TypeScript packages.
// Consuming packages install `eslint` + `typescript-eslint` themselves
// and call this factory with their own `typescript-eslint` import —
// the package cannot resolve it from here (workspace layout).
export default function createBaseConfig(tseslint) {
  return [
    ...tseslint.configs.recommended,
    {
      rules: {
        "@typescript-eslint/no-unused-vars": [
          "error",
          { argsIgnorePattern: "^_" },
        ],
      },
    },
  ];
}
