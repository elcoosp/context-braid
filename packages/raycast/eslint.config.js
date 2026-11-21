const { defineConfig } = require("eslint/config");
const raycastConfig = require("@raycast/eslint-config");

module.exports = defineConfig([
  {
    ignores: ["src/locales/**/messages.ts"],
  },
  ...raycastConfig,
]);
