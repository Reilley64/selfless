const { getDefaultConfig } = require("expo/metro-config");
const { createProxyMiddleware } = require("http-proxy-middleware");
const { withNativewind } = require("nativewind/metro");

const config = getDefaultConfig(__dirname);
const proxy = createProxyMiddleware({
  target: `http://localhost:${process.env.HTTP__PORT ?? "8080"}`,
  pathFilter: ["/actuator", "/api"],
});

config.server.enhanceMiddleware = (middleware) => (request, response, next) =>
  proxy(request, response, () => middleware(request, response, next));

module.exports = withNativewind(config, {
  globalClassNamePolyfill: false,
  disableTypeScriptGeneration: true,
});
