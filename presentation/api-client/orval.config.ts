import { defineConfig } from "orval";

export default defineConfig({
  selfless: {
    input: {
      target: "../../openapi.yaml",
      filters: { mode: "exclude", tags: ["telemetry"] },
    },
    output: {
      target: "src/generated/selfless.ts",
      client: "react-query",
      httpClient: "fetch",
      override: {
        mutator: { path: "src/tracedFetch.ts", name: "tracedFetch" },
      },
    },
  },
});
