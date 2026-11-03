const resolve = require("@react-native/jest-preset/jest/resolver.js");

module.exports = (path, options) =>
  resolve(path, { ...options, conditions: options.conditions?.filter((condition) => condition !== "node") });
