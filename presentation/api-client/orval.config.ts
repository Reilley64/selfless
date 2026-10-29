import { defineConfig } from "orval";

export default defineConfig({
  selfless: {
    input: "../../openapi.yaml",
    output: {
      target: "src/generated/selfless.ts",
      client: "react-query",
      httpClient: "fetch",
    },
  },
});
