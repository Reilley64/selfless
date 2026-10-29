const { defineConfig } = require("eslint/config");
const expoConfig = require("eslint-config-expo/flat");

module.exports = defineConfig([
  expoConfig,
  {
    ignores: ["target/", "**/dist/", "**/.expo/", "presentation/api-client/src/generated/"],
  },
]);
