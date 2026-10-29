// PROTOTYPE artwork: generated SVG stand-ins for posters and backdrops, copied from presentation/tv/prototype.
import { rng, type Title, type User } from "./prototype-data";

const cache = new Map<string, string>();
const fonts = [
  `font-family="Avenir Next, Helvetica Neue, Arial, sans-serif" font-weight="800"`,
  `font-family="Georgia, Times New Roman, serif" font-style="italic" font-weight="700"`,
  `font-family="Futura, Trebuchet MS, sans-serif" font-weight="700" letter-spacing="4"`,
];

function esc(s: string) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/'/g, "&apos;");
}

function wrap(text: string, max: number) {
  const lines: string[] = [];
  let line = "";
  for (const word of text.split(" ")) {
    if (line && (line + " " + word).length > max) {
      lines.push(line);
      line = word;
    } else line = line ? line + " " + word : word;
  }
  lines.push(line);
  return lines;
}

function shapes(t: Title, w: number, h: number, seed: number) {
  const r = rng(seed);
  const light = `hsla(${t.hue2},90%,75%,`;
  let out = "";
  switch (t.pattern) {
    case 0:
      out += `<circle cx="${w * (0.55 + r() * 0.3)}" cy="${h * (0.25 + r() * 0.3)}" r="${h * (0.25 + r() * 0.15)}" fill="${light}0.55)"/>`;
      out += `<circle cx="${w * (0.55 + r() * 0.3)}" cy="${h * 0.4}" r="${h * 0.5}" fill="none" stroke="${light}0.25)" stroke-width="${h * 0.02}"/>`;
      break;
    case 1:
      for (let i = 0; i < 7; i++) {
        const x = w * (0.3 + i * 0.12);
        out += `<rect x="${x}" y="${-h}" width="${w * 0.04}" height="${h * 3}" fill="${light}${0.1 + r() * 0.25})" transform="rotate(25 ${x} ${h / 2})"/>`;
      }
      break;
    case 2: {
      for (let layer = 0; layer < 3; layer++) {
        let d = `M0 ${h}`;
        for (let x = 0; x <= w; x += w / 6) d += ` L${x} ${h * (0.45 + layer * 0.15 + r() * 0.2)}`;
        d += ` L${w} ${h} Z`;
        out += `<path d="${d}" fill="hsla(${t.hue},50%,${8 + layer * 6}%,${0.6 + layer * 0.15})"/>`;
      }
      out += `<circle cx="${w * 0.75}" cy="${h * 0.28}" r="${h * 0.09}" fill="${light}0.8)"/>`;
      break;
    }
    case 3:
      for (let i = 0; i < 26; i++) {
        out += `<circle cx="${r() * w}" cy="${r() * h}" r="${h * (0.01 + r() * 0.05)}" fill="${light}${0.2 + r() * 0.5})"/>`;
      }
      break;
    default:
      for (let i = 0; i < 5; i++) {
        const s = h * (0.2 + r() * 0.5);
        out += `<rect x="${w * (0.35 + r() * 0.5)}" y="${r() * h * 0.7}" width="${s}" height="${s}" fill="none" stroke="${light}${0.2 + r() * 0.4})" stroke-width="${h * 0.015}" transform="rotate(${r() * 90} ${w * 0.7} ${h * 0.4})"/>`;
      }
  }
  return out;
}

function svgUrl(svg: string) {
  return `url("data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}")`;
}

/** Landscape tile artwork with the title painted on, like key art. */
export function tileArt(t: Title, seed = 0): string {
  const key = `tile-${t.id}-${seed}`;
  if (cache.has(key)) return cache.get(key)!;
  const w = 480;
  const h = 270;
  const lines = wrap(t.name.toUpperCase(), t.name.length > 18 ? 13 : 11);
  const size = lines.length > 2 ? 36 : 46;
  const text = lines
    .map((l, i) => `<text x="28" y="${h - 30 - (lines.length - 1 - i) * (size + 4)}" ${fonts[t.font]} font-size="${size}" fill="#fff">${esc(l)}</text>`)
    .join("");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" preserveAspectRatio="xMidYMid slice">
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${t.hue},65%,38%)"/><stop offset="1" stop-color="hsl(${(t.hue + 40) % 360},70%,10%)"/></linearGradient>
<linearGradient id="f" x1="0" y1="0" x2="0" y2="1"><stop offset="0.4" stop-color="#000" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity="0.65"/></linearGradient></defs>
<rect width="${w}" height="${h}" fill="url(#g)"/>${shapes(t, w, h, t.id.length * 7 + Number(t.id.slice(1)) + seed * 101)}
<rect width="${w}" height="${h}" fill="url(#f)"/>${seed ? "" : text}</svg>`;
  const url = svgUrl(svg);
  cache.set(key, url);
  return url;
}

/** Wide backdrop without text, for hero areas and detail screens. */
export function backdropArt(t: Title): string {
  const key = `back-${t.id}`;
  if (cache.has(key)) return cache.get(key)!;
  const w = 1600;
  const h = 900;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" preserveAspectRatio="xMidYMid slice">
<defs><radialGradient id="g" cx="0.7" cy="0.35" r="0.9"><stop offset="0" stop-color="hsl(${t.hue},70%,45%)"/><stop offset="1" stop-color="hsl(${(t.hue + 40) % 360},70%,7%)"/></radialGradient></defs>
<rect width="${w}" height="${h}" fill="url(#g)"/>${shapes(t, w, h, Number(t.id.slice(1)) * 11 + 5)}</svg>`;
  const url = svgUrl(svg);
  cache.set(key, url);
  return url;
}

export function avatarArt(u: User): string {
  const s = 200;
  const marks = [
    `<circle cx="100" cy="100" r="46" fill="none" stroke="#fff" stroke-width="14"/><circle cx="100" cy="100" r="12" fill="#fff"/>`,
    `<path d="M50 140 L100 55 L150 140 Z" fill="none" stroke="#fff" stroke-width="14" stroke-linejoin="round"/>`,
    `<path d="M100 48 L113 86 L152 86 L120 110 L132 150 L100 126 L68 150 L80 110 L48 86 L87 86 Z" fill="#fff"/>`,
    `<path d="M45 100 Q72 55 100 100 T155 100" fill="none" stroke="#fff" stroke-width="14" stroke-linecap="round"/><path d="M45 135 Q72 90 100 135 T155 135" fill="none" stroke="#fff" stroke-opacity="0.5" stroke-width="10" stroke-linecap="round"/>`,
  ];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${s} ${s}"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${u.color}"/><stop offset="1" stop-color="#111"/></linearGradient></defs><rect width="${s}" height="${s}" fill="url(#g)"/>${marks[u.shape]}</svg>`;
  return svgUrl(svg);
}

export const icons = {
  search: `<svg viewBox="0 0 24 24"><circle cx="10.5" cy="10.5" r="6.5" fill="none" stroke="currentColor" stroke-width="2.4"/><path d="M15.5 15.5 L21 21" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"/></svg>`,
  home: `<svg viewBox="0 0 24 24"><path d="M3 11 L12 3.5 L21 11 V20.5 H14.5 V14.5 H9.5 V20.5 H3 Z" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"/></svg>`,
  shows: `<svg viewBox="0 0 24 24"><rect x="2.5" y="5.5" width="19" height="13" rx="2" fill="none" stroke="currentColor" stroke-width="2.2"/><path d="M8 22 H16 M8.5 1.8 L12 5.3 L15.5 1.8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>`,
  movies: `<svg viewBox="0 0 24 24"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="currentColor" stroke-width="2.2"/><path d="M7.5 3 V21 M16.5 3 V21 M3 8 H7.5 M3 12 H7.5 M3 16 H7.5 M16.5 8 H21 M16.5 12 H21 M16.5 16 H21" stroke="currentColor" stroke-width="1.8"/></svg>`,
  play: `<svg viewBox="0 0 24 24"><path d="M6 3.5 L20 12 L6 20.5 Z" fill="currentColor"/></svg>`,
  info: `<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9.5" fill="none" stroke="currentColor" stroke-width="2.2"/><path d="M12 10.5 V17" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/><circle cx="12" cy="7.2" r="1.5" fill="currentColor"/></svg>`,
  check: `<svg viewBox="0 0 24 24"><path d="M4.5 12.5 L9.5 17.5 L19.5 6.5" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/></svg>`,
  restart: `<svg viewBox="0 0 24 24"><path d="M5 12 A7 7 0 1 0 8 6.3" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round"/><path d="M8.5 2.5 V7 H4" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round"/></svg>`,
};
