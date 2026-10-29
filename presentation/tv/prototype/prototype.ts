// PROTOTYPE: five variants of the selfless TV app home, switchable with ?variant=A|B|C|D|E.
// Question: which pre-2025-style TV layout (left menu + rows) should the TV app use?
// Verdict so far: E (B's billboard, then A's focused details once focus is in the rows), so E is the default.
// ?user=maya skips the user picker; ?focus=<row>,<col> focuses that tile on the home of variant D or E.
// Throwaway code. No tests, no persistence, fake data only.
import {
  episodeLabel, featuredFor, metaLine, moreLike, nextUp, resumeOf, rowsFor, titles, users,
  type Episode, type Page, type Row, type Tile, type Title, type User,
} from "./prototype-data";
import { avatarArt, backdropArt, icons, tileArt } from "./prototype-art";

type Key = "up" | "down" | "left" | "right" | "select" | "back";
type VariantKey = "A" | "B" | "C" | "D" | "E";

interface Screen {
  el: HTMLElement;
  key(k: Key): boolean | void;
  describe(): string;
}

const variants: Record<VariantKey, string> = { A: "Focused details", B: "Billboard", C: "Expanding tile", D: "Billboard + expanding tile", E: "Billboard then focused details" };
const variantKeys = Object.keys(variants) as VariantKey[];
const params = new URLSearchParams(location.search);
let variant: VariantKey = variantKeys.includes(params.get("variant") as VariantKey) ? (params.get("variant") as VariantKey) : "E";

function h<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", html = ""): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  if (cls) el.className = cls;
  if (html) el.innerHTML = html;
  return el;
}

const clamp = (n: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, n));

// ---------- navigation stack ----------

const stage = document.getElementById("stage")!;
let stack: Screen[] = [];

function show() {
  stage.replaceChildren(stack[stack.length - 1].el);
  renderSwitcher();
}
function push(s: Screen) {
  stack.push(s);
  show();
}
function back() {
  if (stack.length > 1) {
    stack.pop();
    show();
  }
}
function replaceAll(s: Screen) {
  stack = [s];
  show();
}

// ---------- shared pieces ----------

function tileEl(t: Tile, caption = false) {
  const el = h("div", "tile");
  const art = h("div", "tile-art");
  art.style.backgroundImage = tileArt(t.title);
  el.append(art);
  if (t.episode) el.append(h("div", "tile-badge", episodeLabel(t.episode)));
  if (t.progress != null) el.append(h("div", "tile-progress", `<i style="width:${t.progress * 100}%"></i>`));
  if (caption) el.append(h("div", "tile-caption", t.title.name));
  return el;
}

function metaHtml(t: Title) {
  return metaLine(t).map((m, i) => (i === 1 ? `<span class="rating">${m}</span>` : `<span>${m}</span>`)).join("");
}

function minutesLeft(t: Tile) {
  const total = t.episode ? t.episode.runtime : t.title.runtime;
  return Math.round(total * (1 - (t.progress ?? 0)));
}

function tileSummary(t: Tile) {
  const ep = t.episode
    ? `<div class="ep-line"><b>${episodeLabel(t.episode)}</b> ${t.episode.name}</div>`
    : "";
  const resume = t.progress != null
    ? `<div class="resume"><div class="bar"><i style="width:${t.progress * 100}%"></i></div><span>${minutesLeft(t)} min left</span></div>`
    : "";
  return `<div class="kicker">${t.title.kind === "movie" ? "Movie" : "Show"}</div>
    <h1>${t.title.name}</h1>
    <div class="meta">${metaHtml(t.title)}</div>
    ${ep}${resume}
    <p class="synopsis">${t.episode ? t.episode.synopsis : t.title.synopsis}</p>`;
}

/** Adds the backdrop and card that a tile shows once it expands (variants C and D). */
function addExpandedCard(el: HTMLElement, t: Tile) {
  const back = h("div", "tile-back");
  back.style.backgroundImage = backdropArt(t.title);
  const card = h("div", "card-info", tileSummary(t));
  card.append(h("div", "buttons", `<div class="btn primary">${icons.play}<span>${t.progress != null ? "Resume" : "Play"}</span></div><div class="btn">${icons.info}<span>More info</span></div>`));
  el.prepend(back);
  el.append(card);
}

function openTitle(t: Tile) {
  push(t.title.kind === "movie" ? new MovieDetail(t.title) : new ShowDetail(t.title));
}

function play(title: Title, episode?: Episode, progress = 0) {
  push(new Player(title, episode, progress));
}

function playTile(t: Tile) {
  if (t.title.kind === "show" && !t.episode) {
    const r = currentUser ? resumeOf(currentUser, t.title) : undefined;
    play(t.title, r?.episode ?? t.title.seasons[0].episodes[0], r?.progress);
  } else play(t.title, t.episode, t.progress);
}

/** A horizontally scrolling row whose focused tile stays anchored at the left edge. */
class RowStrip {
  el = h("div", "row");
  track = h("div", "track");
  tiles: HTMLElement[];
  col = 0;
  constructor(public row: Row, public step: number, caption = false) {
    this.el.append(h("div", "row-label", row.name), this.track);
    this.tiles = row.tiles.map((t) => tileEl(t, caption));
    this.track.append(...this.tiles);
  }
  get tile() {
    return this.row.tiles[this.col];
  }
  move(d: number) {
    const next = clamp(this.col + d, 0, this.tiles.length - 1);
    const moved = next !== this.col;
    this.col = next;
    return moved;
  }
  layout(focused: boolean) {
    this.el.classList.toggle("focused", focused);
    this.track.style.transform = `translateX(${-this.col * this.step}px)`;
    this.tiles.forEach((el, i) => {
      el.classList.toggle("focus", focused && i === this.col);
      el.classList.toggle("after", focused && i > this.col);
      el.classList.toggle("before", i < this.col);
    });
  }
}

interface PageView {
  el: HTMLElement;
  key(k: Key): boolean;
  describe(): string;
  active(on: boolean): void;
}

// ---------- Variant A: Focused details ----------

class VariantA implements PageView {
  el = h("div", "page va");
  backs = [h("div", "va-back"), h("div", "va-back")];
  front = 0;
  info = h("div", "va-info");
  inner = h("div", "va-rows-inner");
  strips: RowStrip[];
  row = 0;
  shown?: Tile;
  constructor(rows: Row[]) {
    const hero = h("div", "va-hero");
    hero.append(...this.backs, h("div", "va-fade"), this.info);
    const viewport = h("div", "va-rows");
    viewport.append(this.inner);
    this.strips = rows.map((r) => new RowStrip(r, 355));
    this.inner.append(...this.strips.map((s) => s.el));
    this.el.append(hero, viewport);
    this.update();
  }
  get tile() {
    return this.strips[this.row].tile;
  }
  update() {
    this.inner.style.transform = `translateY(${-this.row * 290}px)`;
    this.strips.forEach((s, i) => {
      s.layout(i === this.row);
      s.el.classList.toggle("above", i < this.row);
    });
    const t = this.tile;
    if (t === this.shown) return;
    this.shown = t;
    const next = this.backs[1 - this.front];
    next.style.backgroundImage = backdropArt(t.title);
    next.classList.add("on");
    this.backs[this.front].classList.remove("on");
    this.front = 1 - this.front;
    this.info.innerHTML = tileSummary(t);
    this.info.animate([{ opacity: 0, transform: "translateY(12px)" }, { opacity: 1, transform: "none" }], { duration: 260, easing: "ease-out" });
  }
  key(k: Key) {
    const s = this.strips[this.row];
    if (k === "left" && !s.move(-1)) return false;
    if (k === "right") s.move(1);
    if (k === "up") this.row = clamp(this.row - 1, 0, this.strips.length - 1);
    if (k === "down") this.row = clamp(this.row + 1, 0, this.strips.length - 1);
    if (k === "select") openTitle(this.tile);
    if (k === "back") {
      if (this.row === 0 && s.col === 0) return false;
      this.row = 0;
      this.strips.forEach((x) => (x.col = 0));
    }
    this.update();
    return true;
  }
  active(on: boolean) {
    this.el.classList.toggle("inactive", !on);
  }
  describe() {
    return `row "${this.strips[this.row].row.name}", tile ${this.strips[this.row].col + 1}: ${this.tile.title.name}`;
  }
}

// ---------- Variant B: Billboard ----------

class VariantB implements PageView {
  el = h("div", "page vb");
  scroller = h("div", "vb-scroller");
  buttons: HTMLElement[];
  strips: RowStrip[];
  row = -1;
  btn = 0;
  constructor(rows: Row[], public featured: Title) {
    const bill = h("div", "vb-billboard");
    const back = h("div", "vb-back");
    back.style.backgroundImage = backdropArt(featured);
    const info = h("div", "vb-info", `<div class="kicker">Featured ${featured.kind === "movie" ? "Movie" : "Show"}</div>
      <h1 class="vb-title">${featured.name}</h1>
      <div class="meta">${metaHtml(featured)}</div>
      <p class="synopsis">${featured.synopsis}</p>`);
    const btns = h("div", "buttons");
    this.buttons = [h("div", "btn primary", `${icons.play}<span>Play</span>`), h("div", "btn", `${icons.info}<span>More info</span>`)];
    btns.append(...this.buttons);
    info.append(btns);
    bill.append(back, h("div", "vb-fade"), info);
    const rowsEl = h("div", "vb-rows");
    this.strips = rows.map((r) => new RowStrip(r, 320, true));
    rowsEl.append(...this.strips.map((s) => s.el));
    this.scroller.append(bill, rowsEl);
    this.el.append(this.scroller);
    this.update();
  }
  update() {
    const y = this.row < 0 ? 0 : -(720 + this.row * 380) + 70;
    this.scroller.style.transform = `translateY(${y}px)`;
    this.el.classList.toggle("in-rows", this.row >= 0);
    this.buttons.forEach((b, i) => b.classList.toggle("focus", this.row < 0 && i === this.btn));
    this.strips.forEach((s, i) => {
      s.layout(i === this.row);
      s.el.classList.toggle("above", i < this.row);
    });
  }
  key(k: Key) {
    if (this.row < 0) {
      if (k === "left" && this.btn === 0) return false;
      if (k === "left" || k === "right") this.btn = k === "left" ? 0 : 1;
      if (k === "down") this.row = 0;
      if (k === "back") return false;
      if (k === "select") {
        if (this.btn === 0) playTile({ title: this.featured });
        else openTitle({ title: this.featured });
      }
    } else {
      const s = this.strips[this.row];
      if (k === "left" && !s.move(-1)) return false;
      if (k === "right") s.move(1);
      if (k === "up") this.row -= 1;
      if (k === "down") this.row = clamp(this.row + 1, 0, this.strips.length - 1);
      if (k === "select") openTitle(s.tile);
      if (k === "back") {
        this.row = -1;
        this.strips.forEach((x) => (x.col = 0));
      }
    }
    this.update();
    return true;
  }
  active(on: boolean) {
    this.el.classList.toggle("inactive", !on);
  }
  describe() {
    if (this.row < 0) return `billboard "${this.featured.name}", button ${this.btn ? "More info" : "Play"}`;
    const s = this.strips[this.row];
    return `row "${s.row.name}", tile ${s.col + 1}: ${s.tile.title.name}`;
  }
}

// ---------- Variant C: Expanding tile ----------

class VariantC implements PageView {
  el = h("div", "page vc");
  inner = h("div", "vc-rows-inner");
  strips: RowStrip[];
  row = 0;
  open = false;
  inCard = false;
  btn = 0;
  timer = 0;
  constructor(rows: Row[], pageName: string) {
    this.el.append(h("div", "vc-header", `<h2>${pageName}</h2>`));
    const viewport = h("div", "vc-rows");
    viewport.append(this.inner);
    this.el.append(viewport);
    this.strips = rows.map((r) => new RowStrip(r, 268));
    for (const s of this.strips) s.tiles.forEach((el, i) => addExpandedCard(el, s.row.tiles[i]));
    this.inner.append(...this.strips.map((s) => s.el));
    this.update(true);
  }
  get strip() {
    return this.strips[this.row];
  }
  update(moved: boolean) {
    if (moved) {
      this.open = false;
      this.inCard = false;
      clearTimeout(this.timer);
      this.timer = window.setTimeout(() => {
        this.open = true;
        this.render();
      }, 450);
    }
    this.render();
  }
  render() {
    this.inner.style.transform = `translateY(${-this.row * 222}px)`;
    this.strips.forEach((s, i) => {
      s.layout(i === this.row);
      s.el.classList.toggle("above", i < this.row);
      s.el.classList.toggle("open", i === this.row && this.open);
      s.tiles.forEach((el, j) => {
        const isOpen = i === this.row && j === s.col && this.open;
        el.classList.toggle("open", isOpen);
        el.classList.toggle("in-card", isOpen && this.inCard);
        el.querySelectorAll(".card-info .btn").forEach((b, n) => b.classList.toggle("focus", isOpen && this.inCard && n === this.btn));
      });
    });
    renderSwitcher();
  }
  key(k: Key) {
    if (this.inCard) {
      if (k === "left" || k === "right") this.btn = k === "left" ? 0 : 1;
      if (k === "select") {
        if (this.btn === 0) playTile(this.strip.tile);
        else openTitle(this.strip.tile);
      }
      if (k === "back" || k === "up" || k === "down") this.inCard = false;
      if (k === "up" || k === "down") return this.key(k);
      this.render();
      return true;
    }
    const s = this.strip;
    if (k === "left") {
      if (!s.move(-1)) return false;
      this.update(true);
    }
    if (k === "right") this.update(s.move(1));
    if (k === "up" || k === "down") {
      const next = clamp(this.row + (k === "up" ? -1 : 1), 0, this.strips.length - 1);
      const moved = next !== this.row;
      this.row = next;
      this.update(moved);
    }
    if (k === "select") {
      clearTimeout(this.timer);
      this.open = true;
      this.inCard = true;
      this.btn = 0;
      this.render();
    }
    if (k === "back") {
      if (this.row === 0 && s.col === 0) return false;
      this.row = 0;
      this.strips.forEach((x) => (x.col = 0));
      this.update(true);
    }
    return true;
  }
  active(on: boolean) {
    this.el.classList.toggle("inactive", !on);
  }
  describe() {
    const s = this.strip;
    return `row "${s.row.name}", tile ${s.col + 1}: ${s.tile.title.name}${this.open ? " (expanded)" : ""}${this.inCard ? `, button ${this.btn ? "More info" : "Play"}` : ""}`;
  }
}

// ---------- Variant D: Billboard + expanding tile ----------

/** B's billboard on top; the rows below expand the focused tile in place like C. */
class VariantD implements PageView {
  el = h("div", "page vd");
  scroller = h("div", "vd-scroller");
  buttons: HTMLElement[];
  strips: RowStrip[];
  row = -1;
  btn = 0;
  open = false;
  inCard = false;
  cardBtn = 0;
  timer = 0;
  rowHeight: number;
  rowsTop: number;
  constructor(rows: Row[], public featured: Title) {
    const css = getComputedStyle(document.documentElement);
    const token = (name: string) => parseFloat(css.getPropertyValue(name));
    this.rowHeight = token("--tile-tv-height") + 100;
    this.rowsTop = token("--billboard-tv-height") - 200;
    const bill = h("div", "vb-billboard");
    const back = h("div", "vb-back");
    back.style.backgroundImage = backdropArt(featured);
    const info = h("div", "vb-info", `<div class="kicker">Featured ${featured.kind === "movie" ? "Movie" : "Show"}</div>
      <h1 class="vb-title">${featured.name}</h1>
      <div class="meta">${metaHtml(featured)}</div>
      <p class="synopsis">${featured.synopsis}</p>`);
    const btns = h("div", "buttons");
    this.buttons = [h("div", "btn primary", `${icons.play}<span>Play</span>`), h("div", "btn", `${icons.info}<span>More info</span>`)];
    btns.append(...this.buttons);
    info.append(btns);
    bill.append(back, h("div", "vb-fade"), info);
    const rowsEl = h("div", "vd-rows");
    this.strips = rows.map((r) => new RowStrip(r, token("--tile-tv-width") + token("--tile-tv-gap")));
    for (const s of this.strips) s.tiles.forEach((el, i) => addExpandedCard(el, s.row.tiles[i]));
    rowsEl.append(...this.strips.map((s) => s.el));
    this.scroller.append(bill, rowsEl);
    this.el.append(this.scroller);
    this.render();
  }
  get strip() {
    return this.strips[this.row];
  }
  focusAt(row: number, col: number) {
    this.row = clamp(row, 0, this.strips.length - 1);
    this.strip.col = clamp(col, 0, this.strip.tiles.length - 1);
    this.open = true;
    this.render();
  }
  update(moved: boolean) {
    if (moved) {
      this.open = false;
      this.inCard = false;
      clearTimeout(this.timer);
      if (this.row >= 0) {
        this.timer = window.setTimeout(() => {
          this.open = true;
          this.render();
        }, 450);
      }
    }
    this.render();
  }
  render() {
    // Rows above the focused one are always collapsed, so the focused row settles near the top of the screen.
    const y = this.row < 0 ? 0 : -(this.rowsTop + this.row * this.rowHeight) + 90;
    this.scroller.style.transform = `translateY(${y}px)`;
    this.buttons.forEach((b, i) => b.classList.toggle("focus", this.row < 0 && i === this.btn));
    this.strips.forEach((s, i) => {
      s.layout(i === this.row);
      s.el.classList.toggle("above", i < this.row);
      s.el.classList.toggle("open", i === this.row && this.open);
      s.tiles.forEach((el, j) => {
        const isOpen = i === this.row && j === s.col && this.open;
        el.classList.toggle("open", isOpen);
        el.classList.toggle("in-card", isOpen && this.inCard);
        el.querySelectorAll(".card-info .btn").forEach((b, n) => b.classList.toggle("focus", isOpen && this.inCard && n === this.cardBtn));
      });
    });
    renderSwitcher();
  }
  key(k: Key): boolean {
    if (this.row < 0) {
      if (k === "back" || (k === "left" && this.btn === 0)) return false;
      if (k === "left" || k === "right") this.btn = k === "left" ? 0 : 1;
      if (k === "select") {
        if (this.btn === 0) playTile({ title: this.featured });
        else openTitle({ title: this.featured });
      }
      if (k === "down") this.row = 0;
      this.update(k === "down");
      return true;
    }
    if (this.inCard) {
      if (k === "left" || k === "right") this.cardBtn = k === "left" ? 0 : 1;
      if (k === "select") {
        if (this.cardBtn === 0) playTile(this.strip.tile);
        else openTitle(this.strip.tile);
      }
      if (k === "back" || k === "up" || k === "down") this.inCard = false;
      if (k === "up" || k === "down") return this.key(k);
      this.render();
      return true;
    }
    const s = this.strip;
    if (k === "left") {
      if (!s.move(-1)) return false;
      this.update(true);
    }
    if (k === "right") this.update(s.move(1));
    if (k === "up") {
      this.row -= 1;
      this.update(true);
    }
    if (k === "down") {
      const next = clamp(this.row + 1, 0, this.strips.length - 1);
      const moved = next !== this.row;
      this.row = next;
      this.update(moved);
    }
    if (k === "select") {
      clearTimeout(this.timer);
      this.open = true;
      this.inCard = true;
      this.cardBtn = 0;
      this.render();
    }
    if (k === "back") {
      this.row = -1;
      this.strips.forEach((x) => (x.col = 0));
      this.update(true);
    }
    return true;
  }
  active(on: boolean) {
    this.el.classList.toggle("inactive", !on);
  }
  describe() {
    if (this.row < 0) return `billboard "${this.featured.name}", button ${this.btn ? "More info" : "Play"}`;
    const s = this.strip;
    return `row "${s.row.name}", tile ${s.col + 1}: ${s.tile.title.name}${this.open ? " (expanded)" : ""}${this.inCard ? `, button ${this.cardBtn ? "More info" : "Play"}` : ""}`;
  }
}

// ---------- Variant E: Billboard then focused details ----------

/** Opens on B's billboard; once focus is in the rows, the top area becomes A's focused-details area. Tiles never expand. */
class VariantE implements PageView {
  el = h("div", "page ve");
  bill = h("div", "vb-billboard ve-bill");
  hero = h("div", "va-hero ve-hero");
  backs = [h("div", "va-back"), h("div", "va-back")];
  front = 0;
  info = h("div", "va-info");
  viewport = h("div", "ve-rows");
  inner = h("div", "ve-rows-inner");
  buttons: HTMLElement[];
  strips: RowStrip[];
  row = -1;
  btn = 0;
  shown?: Tile;
  rowHeight: number;
  constructor(rows: Row[], public featured: Title) {
    const css = getComputedStyle(document.documentElement);
    const token = (name: string) => parseFloat(css.getPropertyValue(name));
    this.rowHeight = token("--tile-tv-height") + 100;
    const back = h("div", "vb-back");
    back.style.backgroundImage = backdropArt(featured);
    const info = h("div", "vb-info", `<div class="kicker">Featured ${featured.kind === "movie" ? "Movie" : "Show"}</div>
      <h1 class="vb-title">${featured.name}</h1>
      <div class="meta">${metaHtml(featured)}</div>
      <p class="synopsis">${featured.synopsis}</p>`);
    const btns = h("div", "buttons");
    this.buttons = [h("div", "btn primary", `${icons.play}<span>Play</span>`), h("div", "btn", `${icons.info}<span>More info</span>`)];
    btns.append(...this.buttons);
    info.append(btns);
    this.bill.append(back, h("div", "vb-fade"), info);
    this.hero.append(...this.backs, h("div", "va-fade"), this.info);
    this.strips = rows.map((r) => new RowStrip(r, token("--tile-tv-width") + token("--tile-tv-gap")));
    this.inner.append(...this.strips.map((s) => s.el));
    this.viewport.append(this.inner);
    this.el.append(this.bill, this.hero, this.viewport);
    this.render();
  }
  focusAt(row: number, col: number) {
    this.row = clamp(row, 0, this.strips.length - 1);
    this.strips[this.row].col = clamp(col, 0, this.strips[this.row].tiles.length - 1);
    this.render();
  }
  render() {
    const inRows = this.row >= 0;
    this.el.classList.toggle("in-rows", inRows);
    this.inner.style.transform = `translateY(${-Math.max(0, this.row) * this.rowHeight}px)`;
    this.buttons.forEach((b, i) => b.classList.toggle("focus", !inRows && i === this.btn));
    this.strips.forEach((s, i) => {
      s.layout(i === this.row);
      s.el.classList.toggle("above", i < this.row);
    });
    if (!inRows) {
      this.shown = undefined;
      return;
    }
    const t = this.strips[this.row].tile;
    if (t === this.shown) return;
    this.shown = t;
    const next = this.backs[1 - this.front];
    next.style.backgroundImage = backdropArt(t.title);
    next.classList.add("on");
    this.backs[this.front].classList.remove("on");
    this.front = 1 - this.front;
    this.info.innerHTML = tileSummary(t);
    this.info.animate([{ opacity: 0, transform: "translateY(12px)" }, { opacity: 1, transform: "none" }], { duration: 260, easing: "ease-out" });
  }
  key(k: Key) {
    if (this.row < 0) {
      if (k === "back" || (k === "left" && this.btn === 0)) return false;
      if (k === "left" || k === "right") this.btn = k === "left" ? 0 : 1;
      if (k === "down") this.row = 0;
      if (k === "select") {
        if (this.btn === 0) playTile({ title: this.featured });
        else openTitle({ title: this.featured });
      }
    } else {
      const s = this.strips[this.row];
      if (k === "left" && !s.move(-1)) return false;
      if (k === "right") s.move(1);
      if (k === "up") this.row -= 1;
      if (k === "down") this.row = clamp(this.row + 1, 0, this.strips.length - 1);
      if (k === "select") openTitle(s.tile);
      if (k === "back") {
        this.row = -1;
        this.strips.forEach((x) => (x.col = 0));
      }
    }
    this.render();
    return true;
  }
  active(on: boolean) {
    this.el.classList.toggle("inactive", !on);
  }
  describe() {
    if (this.row < 0) return `billboard "${this.featured.name}", button ${this.btn ? "More info" : "Play"}`;
    const s = this.strips[this.row];
    return `row "${s.row.name}", tile ${s.col + 1}: ${s.tile.title.name}`;
  }
}

// ---------- Search (shared) ----------

class SearchView implements PageView {
  el = h("div", "page search");
  keys = [..."abcdefghijklmnopqrstuvwxyz", "space", "delete"];
  query = "";
  zone: "keys" | "results" = "keys";
  k = 0;
  r = 0;
  queryEl = h("div", "search-query");
  keyEls: HTMLElement[];
  resultsEl = h("div", "search-results");
  resultsTitle = h("h3");
  results: Title[] = [];
  resultEls: HTMLElement[] = [];
  constructor() {
    const left = h("div", "search-left");
    const grid = h("div", "search-keys");
    this.keyEls = this.keys.map((c) => h("div", `key${c.length > 1 ? " word" : ""}`, c === "space" ? "Space" : c === "delete" ? "⌫" : c));
    grid.append(...this.keyEls);
    left.append(this.queryEl, grid);
    const right = h("div", "search-right");
    right.append(this.resultsTitle, this.resultsEl);
    this.el.append(left, right);
    this.search();
  }
  search() {
    const q = this.query.trim().toLowerCase();
    this.results = q ? titles.filter((t) => t.name.toLowerCase().includes(q)) : [...titles].sort((a, b) => b.popularity - a.popularity).slice(0, 12);
    this.resultsTitle.textContent = q ? `Movies and shows matching "${this.query}"` : "Popular now";
    this.resultEls = this.results.map((title) => tileEl({ title }));
    this.resultsEl.replaceChildren(...this.resultEls);
    this.r = 0;
    this.render();
  }
  render() {
    this.queryEl.innerHTML = this.query ? `${this.query}<i class="caret"></i>` : `<span class="placeholder">Search movies and shows</span><i class="caret"></i>`;
    this.keyEls.forEach((el, i) => el.classList.toggle("focus", this.zone === "keys" && i === this.k));
    this.resultEls.forEach((el, i) => el.classList.toggle("focus", this.zone === "results" && i === this.r));
    const rowOfFocus = Math.floor(this.r / 4);
    this.resultsEl.style.transform = `translateY(${-Math.max(0, rowOfFocus - 1) * 181}px)`;
  }
  key(k: Key) {
    if (this.zone === "keys") {
      const col = this.k % 6;
      if (k === "left") {
        if (col === 0) return false;
        this.k -= 1;
      }
      if (k === "right") {
        if (col === 5 || this.k === this.keys.length - 1) {
          if (this.results.length) this.zone = "results";
        } else this.k += 1;
      }
      if (k === "up" && this.k >= 6) this.k -= 6;
      if (k === "down") this.k = Math.min(this.k + 6, this.keys.length - 1);
      if (k === "select") {
        const c = this.keys[this.k];
        this.query = c === "delete" ? this.query.slice(0, -1) : this.query + (c === "space" ? " " : c);
        this.search();
      }
      if (k === "back") {
        if (!this.query) return false;
        this.query = this.query.slice(0, -1);
        this.search();
      }
    } else {
      const col = this.r % 4;
      if (k === "left") {
        if (col === 0) this.zone = "keys";
        else this.r -= 1;
      }
      if (k === "right" && col < 3) this.r = Math.min(this.r + 1, this.results.length - 1);
      if (k === "up" && this.r >= 4) this.r -= 4;
      if (k === "down") this.r = Math.min(this.r + 4, this.results.length - 1);
      if (k === "select") openTitle({ title: this.results[this.r] });
      if (k === "back") this.zone = "keys";
    }
    this.render();
    return true;
  }
  active(on: boolean) {
    this.el.classList.toggle("inactive", !on);
  }
  describe() {
    return this.zone === "keys" ? `keyboard "${this.keys[this.k]}", query "${this.query}"` : `result ${this.r + 1}: ${this.results[this.r].name}`;
  }
}

// ---------- Shell: left menu + page ----------

let currentUser: User | undefined;

const menuItems: { id: Page | "user"; label: string }[] = [
  { id: "user", label: "" },
  { id: "search", label: "Search" },
  { id: "home", label: "Home" },
  { id: "shows", label: "Shows" },
  { id: "movies", label: "Movies" },
];

class Shell implements Screen {
  el = h("div", "shell");
  menu = h("nav", "menu");
  host = h("div", "page-host");
  itemEls: HTMLElement[];
  page: Page = "home";
  view!: PageView;
  zone: "menu" | "content" = "content";
  menuIndex = 2;
  constructor(public user: User) {
    this.itemEls = menuItems.map((m) => {
      const el = h("div", `menu-item${m.id === "user" ? " user" : ""}`);
      if (m.id === "user") {
        el.innerHTML = `<i class="avatar" style="background-image:${avatarArt(user).replace(/"/g, "'")}"></i><span><b>${user.name}</b><small>Switch user</small></span>`;
      } else {
        el.innerHTML = `<i class="icon">${icons[m.id]}</i><span>${m.label}</span>`;
      }
      return el;
    });
    const list = h("div", "menu-list");
    list.append(...this.itemEls);
    this.menu.append(h("div", "menu-scrim"), list);
    this.el.append(this.host, this.menu);
    this.openPage("home");
  }
  openPage(p: Page) {
    this.page = p;
    const name = { home: "Home", shows: "Shows", movies: "Movies", search: "Search" }[p];
    const rows = rowsFor(p, this.user);
    this.view = p === "search" ? new SearchView()
      : variant === "A" ? new VariantA(rows)
      : variant === "B" ? new VariantB(rows, featuredFor(p))
      : variant === "C" ? new VariantC(rows, name)
      : variant === "D" ? new VariantD(rows, featuredFor(p))
      : new VariantE(rows, featuredFor(p));
    this.host.replaceChildren(this.view.el);
    this.render();
  }
  rebuild() {
    this.openPage(this.page);
  }
  render() {
    this.el.classList.toggle("menu-open", this.zone === "menu");
    this.view.active(this.zone === "content");
    this.itemEls.forEach((el, i) => {
      el.classList.toggle("focus", this.zone === "menu" && i === this.menuIndex);
      el.classList.toggle("current", menuItems[i].id === this.page);
    });
    renderSwitcher();
  }
  key(k: Key) {
    if (this.zone === "content") {
      if (this.view.key(k)) return this.render();
      if (k === "left" || k === "back") {
        this.zone = "menu";
        this.menuIndex = menuItems.findIndex((m) => m.id === this.page);
      }
      return this.render();
    }
    if (k === "up") this.menuIndex = clamp(this.menuIndex - 1, 0, menuItems.length - 1);
    if (k === "down") this.menuIndex = clamp(this.menuIndex + 1, 0, menuItems.length - 1);
    if (k === "right") this.zone = "content";
    if (k === "back") return push(new UserPicker());
    if (k === "select") {
      const id = menuItems[this.menuIndex].id;
      if (id === "user") return push(new UserPicker());
      if (id !== this.page) this.openPage(id);
      this.zone = "content";
    }
    this.render();
  }
  describe() {
    return this.zone === "menu" ? `menu "${menuItems[this.menuIndex].label || "Switch user"}"` : `${this.page}: ${this.view.describe()}`;
  }
}

// ---------- User picker ----------

class UserPicker implements Screen {
  el = h("div", "picker");
  i = 0;
  els: HTMLElement[];
  constructor() {
    this.i = currentUser ? users.indexOf(currentUser) : 0;
    const list = h("div", "picker-users");
    this.els = users.map((u) => {
      const el = h("div", "picker-user", `<div class="picker-avatar"></div><div class="picker-name">${u.name}</div>`);
      (el.firstElementChild as HTMLElement).style.backgroundImage = avatarArt(u);
      return el;
    });
    list.append(...this.els);
    this.el.append(h("div", "picker-brand", `<i>S</i>selfless`), h("h1", "", "Who's watching?"), list);
    this.render();
  }
  render() {
    this.els.forEach((el, i) => el.classList.toggle("focus", i === this.i));
  }
  key(k: Key) {
    if (k === "left") this.i = clamp(this.i - 1, 0, users.length - 1);
    if (k === "right") this.i = clamp(this.i + 1, 0, users.length - 1);
    if (k === "select") {
      currentUser = users[this.i];
      return replaceAll(new Shell(currentUser));
    }
    if (k === "back") back();
    this.render();
  }
  describe() {
    return `user picker, focused ${users[this.i].name}`;
  }
}

// ---------- Movie detail ----------

class MovieDetail implements Screen {
  el = h("div", "detail");
  zone: "buttons" | "row" = "buttons";
  btn = 0;
  buttons: { el: HTMLElement; act: () => void }[];
  strip: RowStrip;
  watched = false;
  constructor(public title: Title) {
    const resume = currentUser ? resumeOf(currentUser, title) : undefined;
    const back = h("div", "detail-back");
    back.style.backgroundImage = backdropArt(title);
    const body = h("div", "detail-body", tileSummary(resume ?? { title }));
    const btns = h("div", "buttons");
    const mk = (cls: string, html: string, act: () => void) => ({ el: h("div", `btn ${cls}`, html), act });
    this.buttons = resume
      ? [
          mk("primary", `${icons.play}<span>Resume</span>`, () => play(title, undefined, resume.progress)),
          mk("", `${icons.restart}<span>Start over</span>`, () => play(title)),
        ]
      : [mk("primary", `${icons.play}<span>Play</span>`, () => play(title))];
    const watchedBtn = mk("", `${icons.check}<span>Mark as watched</span>`, () => {
      this.watched = !this.watched;
      watchedBtn.el.innerHTML = `${icons.check}<span>${this.watched ? "Watched" : "Mark as watched"}</span>`;
    });
    this.buttons.push(watchedBtn);
    btns.append(...this.buttons.map((b) => b.el));
    body.append(btns);
    this.strip = new RowStrip({ name: "More like this", tiles: moreLike(title) }, 355);
    const rowWrap = h("div", "detail-row");
    rowWrap.append(this.strip.el);
    this.el.append(back, h("div", "detail-fade"), body, rowWrap);
    this.render();
  }
  render() {
    this.el.classList.toggle("deep", this.zone === "row");
    this.buttons.forEach((b, i) => b.el.classList.toggle("focus", this.zone === "buttons" && i === this.btn));
    this.strip.layout(this.zone === "row");
    renderSwitcher();
  }
  key(k: Key) {
    if (k === "back") return back();
    if (this.zone === "buttons") {
      if (k === "left") this.btn = clamp(this.btn - 1, 0, this.buttons.length - 1);
      if (k === "right") this.btn = clamp(this.btn + 1, 0, this.buttons.length - 1);
      if (k === "down") this.zone = "row";
      if (k === "select") this.buttons[this.btn].act();
    } else {
      if (k === "left") this.strip.move(-1);
      if (k === "right") this.strip.move(1);
      if (k === "up") this.zone = "buttons";
      if (k === "select") openTitle(this.strip.tile);
    }
    this.render();
  }
  describe() {
    return `movie detail "${this.title.name}", ${this.zone === "buttons" ? `button ${this.btn + 1}` : `More like this, tile ${this.strip.col + 1}`}`;
  }
}

// ---------- Show detail ----------

class ShowDetail implements Screen {
  el = h("div", "detail show");
  zone: "buttons" | "seasons" | "episodes" = "buttons";
  btn = 0;
  season = 0;
  ep = 0;
  buttons: HTMLElement[];
  seasonEls: HTMLElement[];
  epList = h("div", "episodes-inner");
  epEls: HTMLElement[] = [];
  resume?: Tile;
  watched = false;
  constructor(public title: Title) {
    this.resume = currentUser ? resumeOf(currentUser, title) ?? nextUp(currentUser).find((t) => t.title === title) : undefined;
    const start = this.resume?.episode ?? title.seasons[0].episodes[0];
    this.season = start.season - 1;
    this.ep = start.number - 1;
    const back = h("div", "detail-back");
    back.style.backgroundImage = backdropArt(title);
    const body = h("div", "detail-body", `<div class="kicker">Show</div><h1>${title.name}</h1><div class="meta">${metaHtml(title)}</div><p class="synopsis">${title.synopsis}</p>`);
    const btns = h("div", "buttons");
    const verb = this.resume?.progress != null ? "Resume" : this.resume ? "Play next" : "Play";
    this.buttons = [
      h("div", "btn primary", `${icons.play}<span>${verb} ${episodeLabel(start)}</span>`),
      h("div", "btn", `${icons.check}<span>Mark as watched</span>`),
    ];
    btns.append(...this.buttons);
    body.append(btns);
    const lower = h("div", "show-lower", `<h2 class="show-heading"><span>${title.name} · </span>Episodes</h2>`);
    const cols = h("div", "show-cols");
    const seasonsEl = h("div", "seasons");
    this.seasonEls = title.seasons.map((s) => h("div", "season", `Season ${s.number}<small>${s.episodes.length} episodes</small>`));
    seasonsEl.append(...this.seasonEls);
    const episodes = h("div", "episodes");
    episodes.append(this.epList);
    cols.append(seasonsEl, episodes);
    lower.append(cols);
    this.el.append(back, h("div", "detail-fade"), body, lower);
    this.fillEpisodes();
    this.render();
  }
  fillEpisodes() {
    const s = this.title.seasons[this.season];
    this.epEls = s.episodes.map((e) => {
      const isResume = this.resume?.episode === e;
      const progress = isResume && this.resume?.progress != null ? `<div class="tile-progress"><i style="width:${this.resume.progress * 100}%"></i></div>` : "";
      const el = h("div", "episode", `<div class="ep-thumb"><span>${e.number}</span>${progress}</div>
        <div class="ep-text"><div class="ep-title">${e.number}. ${e.name}${isResume ? ` <em>${this.resume?.progress != null ? "Resume" : "Next up"}</em>` : ""}</div><div class="ep-runtime">${e.runtime} min</div><p>${e.synopsis}</p></div>`);
      (el.firstElementChild as HTMLElement).style.backgroundImage = tileArt(this.title, e.number + e.season * 20);
      return el;
    });
    this.epList.replaceChildren(...this.epEls);
  }
  render() {
    this.el.classList.toggle("deep", this.zone !== "buttons");
    this.buttons.forEach((b, i) => b.classList.toggle("focus", this.zone === "buttons" && i === this.btn));
    this.seasonEls.forEach((el, i) => {
      el.classList.toggle("selected", i === this.season);
      el.classList.toggle("focus", this.zone === "seasons" && i === this.season);
    });
    this.epEls.forEach((el, i) => el.classList.toggle("focus", this.zone === "episodes" && i === this.ep));
    this.epList.style.transform = `translateY(${-Math.max(0, this.ep - 1) * 196}px)`;
    renderSwitcher();
  }
  key(k: Key) {
    const eps = this.title.seasons[this.season].episodes;
    if (k === "back") {
      if (this.zone === "buttons") return back();
      this.zone = "buttons";
    } else if (this.zone === "buttons") {
      if (k === "left" || k === "right") this.btn = k === "left" ? 0 : 1;
      if (k === "down") this.zone = "episodes";
      if (k === "select") {
        if (this.btn === 0) {
          const e = this.resume?.episode ?? this.title.seasons[0].episodes[0];
          play(this.title, e, this.resume?.progress);
        } else {
          this.watched = !this.watched;
          this.buttons[1].innerHTML = `${icons.check}<span>${this.watched ? "Watched" : "Mark as watched"}</span>`;
        }
      }
    } else if (this.zone === "seasons") {
      if (k === "up" && this.season === 0) this.zone = "buttons";
      else if (k === "up" || k === "down") {
        this.season = clamp(this.season + (k === "up" ? -1 : 1), 0, this.title.seasons.length - 1);
        this.ep = 0;
        this.fillEpisodes();
      }
      if (k === "right" || k === "select") this.zone = "episodes";
    } else {
      if (k === "up") {
        if (this.ep === 0) this.zone = "buttons";
        else this.ep -= 1;
      }
      if (k === "down") this.ep = clamp(this.ep + 1, 0, eps.length - 1);
      if (k === "left") this.zone = "seasons";
      if (k === "select") play(this.title, eps[this.ep], this.resume?.episode === eps[this.ep] ? this.resume.progress : 0);
    }
    this.render();
  }
  describe() {
    const where = this.zone === "buttons" ? `button ${this.btn + 1}` : this.zone === "seasons" ? `Season ${this.season + 1}` : `Season ${this.season + 1}, Episode ${this.ep + 1}`;
    return `show detail "${this.title.name}", ${where}`;
  }
}

// ---------- Player stand-in ----------

class Player implements Screen {
  el = h("div", "player");
  paused = false;
  constructor(public title: Title, public episode?: Episode, progress = 0) {
    this.el.style.backgroundImage = backdropArt(title);
    const total = episode ? episode.runtime : title.runtime;
    const at = Math.round(total * (progress ?? 0));
    this.el.innerHTML = `<div class="player-top"><h2>${title.name}</h2>${episode ? `<div>${episodeLabel(episode)} · ${episode.name}</div>` : ""}</div>
      <div class="player-center">${icons.play}</div>
      <div class="player-note">Prototype: no video. Back to stop.</div>
      <div class="player-bottom"><div class="bar"><i style="width:${(progress ?? 0) * 100}%"></i></div><span>${at}:00 / ${total}:00</span></div>`;
  }
  key(k: Key) {
    if (k === "back") return back();
    if (k === "select") {
      this.paused = !this.paused;
      this.el.classList.toggle("paused", this.paused);
    }
  }
  describe() {
    return `player "${this.title.name}"${this.episode ? ` ${episodeLabel(this.episode)}` : ""}${this.paused ? " (paused)" : ""}`;
  }
}

// ---------- Variant switcher (prototype chrome, not part of the design) ----------

const switcher = document.getElementById("switcher")!;
switcher.innerHTML = `<button data-d="-1" aria-label="Previous variant">‹</button><div class="sw-label"></div><button data-d="1" aria-label="Next variant">›</button><div class="sw-state"></div><div class="sw-keys">Arrows = D-pad · Enter = select · Esc/Backspace = back · [ ] = variant</div>`;
switcher.querySelectorAll("button").forEach((b) => {
  b.addEventListener("mousedown", (e) => e.preventDefault());
  b.addEventListener("click", () => cycle(Number(b.dataset.d)));
});

function cycle(d: number) {
  variant = variantKeys[(variantKeys.indexOf(variant) + d + variantKeys.length) % variantKeys.length];
  const url = new URL(location.href);
  url.searchParams.set("variant", variant);
  history.replaceState(null, "", url);
  for (const s of stack) if (s instanceof Shell) s.rebuild();
  show();
}

function renderSwitcher() {
  switcher.querySelector(".sw-label")!.innerHTML = `<b>${variant}</b> ${variants[variant]}`;
  const top = stack[stack.length - 1];
  switcher.querySelector(".sw-state")!.textContent = `${currentUser ? currentUser.name + " · " : ""}${top ? top.describe() : ""}`;
}

// ---------- input and scaling ----------

const keymap: Record<string, Key> = {
  ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right",
  Enter: "select", " ": "select", Backspace: "back", Escape: "back",
};

window.addEventListener("keydown", (e) => {
  if (e.key === "[" || e.key === "]") {
    cycle(e.key === "[" ? -1 : 1);
    return;
  }
  const k = keymap[e.key];
  if (!k) return;
  e.preventDefault();
  stack[stack.length - 1].key(k);
  renderSwitcher();
});

function fit() {
  const s = Math.min(innerWidth / 1920, innerHeight / 1080);
  stage.style.transform = `translate(${(innerWidth - 1920 * s) / 2}px, ${(innerHeight - 1080 * s) / 2}px) scale(${s})`;
}
window.addEventListener("resize", fit);
fit();

// ?user=<name> skips the user picker, handy for screenshots.
const preset = users.find((u) => u.name.toLowerCase() === params.get("user")?.toLowerCase());
if (preset) {
  currentUser = preset;
  const shell = new Shell(preset);
  const focus = params.get("focus")?.split(",").map(Number);
  if (focus && (shell.view instanceof VariantD || shell.view instanceof VariantE)) shell.view.focusAt(focus[0], focus[1]);
  replaceAll(shell);
} else replaceAll(new UserPicker());
