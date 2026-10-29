const { getDefaultConfig } = require("expo/metro-config");
const { withNativewind } = require("nativewind/metro");

module.exports = withNativewind(getDefaultConfig(__dirname), {
  globalClassNamePolyfill: false,
  disableTypeScriptGeneration: true,
});
