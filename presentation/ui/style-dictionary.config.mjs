const namespaces = {
  color: "color",
  radius: "radius",
  space: "spacing",
  "font.family": "font",
  "font.weight": "font-weight",
  "font.line-height": "leading",
  "font.size": "text",
};

const isAdmin = (token) => token.path[0] === "admin";

export default {
  source: ["../../tokens/*.tokens.json"],
  hooks: {
    transforms: {
      "name/tailwind": {
        type: "name",
        transform: ({ path }) => {
          const prefix = [path.slice(0, 2).join("."), path[0]].find((key) => key in namespaces);
          return prefix
            ? [namespaces[prefix], ...path.slice(prefix.split(".").length)].join("-")
            : ["spacing", ...path].join("-");
        },
      },
      "name/admin": {
        type: "name",
        transform: ({ path }) => path.slice(1).join("-"),
      },
      "fontFamily/first": {
        type: "value",
        filter: (token) => token.$type === "fontFamily",
        transform: (token) => {
          const [first] = [token.original.$value].flat();
          return /\s/.test(first) ? `'${first}'` : first;
        },
      },
    },
  },
  platforms: {
    theme: {
      transformGroup: "css",
      transforms: ["name/tailwind"],
      buildPath: "theme/",
      files: [
        {
          destination: "theme.css",
          format: "css/variables",
          filter: (token) => !isAdmin(token) && token.path[0] !== "symbol",
          options: { selector: "@theme" },
        },
      ],
    },
    symbols: {
      buildPath: "theme/",
      files: [
        {
          destination: "symbols.json",
          format: "json/nested",
          filter: (token) => token.path[0] === "symbol",
        },
      ],
    },
    fonts: {
      transforms: ["name/tailwind", "fontFamily/first"],
      buildPath: "theme/",
      files: [
        {
          destination: "fonts.css",
          format: "css/variables",
          filter: (token) => token.$type === "fontFamily",
          options: { selector: ["@media (display-mode: native)", ":root"] },
        },
      ],
    },
    admin: {
      transformGroup: "css",
      transforms: ["name/admin"],
      buildPath: "theme/",
      files: [{ destination: "admin.css", format: "css/variables", filter: isAdmin }],
    },
  },
};
