// PROTOTYPE accent picker shared by the TV and web prototypes. ?accent=<key> overrides --color-accent; the floating chips switch it.
const accents: Record<string, string> = {
  amber: "#d97706",
  red: "#dc2626",
  crimson: "#be123c",
  violet: "#7c3aed",
  blue: "#2563eb",
  teal: "#0d9488",
  green: "#16a34a",
  gold: "#ca8a04",
};

const params = new URLSearchParams(location.search);
const apply = (key: string) => {
  document.documentElement.style.setProperty("--color-accent", accents[key] ?? accents.red);
  chips.querySelectorAll("button").forEach((b) => b.toggleAttribute("data-on", b.dataset.key === key));
};

const chips = document.createElement("div");
chips.setAttribute("style", "position:fixed;top:12px;right:12px;z-index:9999;display:flex;gap:6px;padding:6px 8px;background:#fff;border-radius:999px;box-shadow:0 4px 16px rgba(0,0,0,.4);font:12px system-ui");
for (const [key, hex] of Object.entries(accents)) {
  const b = document.createElement("button");
  b.dataset.key = key;
  b.title = key;
  b.setAttribute("style", `width:22px;height:22px;border-radius:50%;border:2px solid #fff;outline:2px solid transparent;background:${hex};cursor:pointer`);
  b.onclick = () => {
    params.set("accent", key);
    history.replaceState(null, "", `${location.pathname}?${params}${location.hash}`);
    apply(key);
  };
  chips.append(b);
}
const style = document.createElement("style");
style.textContent = "[data-on]{outline-color:#111 !important}";
document.head.append(style);
document.body.append(chips);
apply(params.get("accent") ?? "red");
