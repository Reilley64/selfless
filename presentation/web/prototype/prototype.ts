// PROTOTYPE: the selfless web app in the visual language of TV variant D (billboard + expanding tile).
// Question: does the chosen TV design carry over to a mouse-driven web app with real URLs?
// Throwaway code. No tests, no persistence, fake data only. Add ?user=maya to the URL to skip sign-in.
import {
  episodeLabel, featuredFor, metaLine, moreLike, movies, nextUp, resumeOf, rowsFor, shows, titles, users,
  type Episode, type Kind, type Row, type Tile, type Title, type User,
} from "./prototype-data";
import { avatarArt, backdropArt, icons, tileArt } from "./prototype-art";

const nav = document.getElementById("nav")!;
const view = document.getElementById("view")!;
const params = new URLSearchParams(location.search);
let user: User | undefined = users.find((u) => u.name.toLowerCase() === params.get("user")?.toLowerCase());
let afterSignIn = "#/";

const css = getComputedStyle(document.documentElement);
const token = (name: string) => parseFloat(css.getPropertyValue(name));
const expandedWidth = token("--tile-web-expanded-width");
const inset = token("--space-web-inset");
const HOVER_DELAY = 400;

function h<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", html = ""): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  if (cls) el.className = cls;
  if (html) el.innerHTML = html;
  return el;
}

const go = (hash: string) => (location.hash = hash);
const detailUrl = (t: Title) => `#/${t.kind}/${t.id}`;
const watchUrl = (t: Title, e?: Episode) => `#/watch/${t.id}${e ? `?s=${e.season}&e=${e.number}` : ""}`;

// ---------- shared pieces ----------

function metaHtml(t: Title) {
  return metaLine(t).map((m, i) => (i === 1 ? `<span class="rating">${m}</span>` : `<span>${m}</span>`)).join("");
}

function minutesLeft(t: Tile) {
  const total = t.episode ? t.episode.runtime : t.title.runtime;
  return Math.round(total * (1 - (t.progress ?? 0)));
}

function resumeHtml(t: Tile) {
  return t.progress != null
    ? `<div class="resume"><div class="bar"><i style="width:${t.progress * 100}%"></i></div><span>${minutesLeft(t)} min left</span></div>`
    : "";
}

/** The episode a Play button starts: the tile's own, the user's resume or next-up episode, or the first one. */
function startOf(t: Tile): Tile {
  if (t.title.kind === "movie" || t.episode) return t;
  const r = user ? resumeOf(user, t.title) ?? nextUp(user).find((x) => x.title === t.title) : undefined;
  return r ?? { title: t.title, episode: t.title.seasons[0].episodes[0] };
}

function button(label: string, icon: string, primary: boolean, act: () => void) {
  const b = h("button", `btn${primary ? " primary" : ""}`, `${icon}<span>${label}</span>`);
  b.type = "button";
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    act();
  });
  return b;
}

/** A tile that grows in place into a card after the pointer rests on it, like focus on TV variant D. */
function tileEl(t: Tile, row: HTMLElement, track: HTMLElement) {
  const el = h("div", "tile");
  el.tabIndex = 0;
  el.setAttribute("role", "link");
  el.setAttribute("aria-label", t.title.name);
  const art = h("div", "tile-art");
  art.style.backgroundImage = tileArt(t.title);
  const back = h("div", "tile-back");
  back.style.backgroundImage = backdropArt(t.title);
  el.append(back, art);
  if (t.episode) el.append(h("div", "tile-badge", episodeLabel(t.episode)));
  if (t.progress != null) el.append(h("div", "tile-progress", `<i style="width:${t.progress * 100}%"></i>`));
  const card = h("div", "card-info", `<h3>${t.title.name}</h3>
    <div class="meta">${metaHtml(t.title)}</div>
    ${t.episode ? `<div class="ep-line"><b>${episodeLabel(t.episode)}</b> ${t.episode.name}</div>` : ""}
    ${resumeHtml(t)}
    <p class="synopsis">${t.episode ? t.episode.synopsis : t.title.synopsis}</p>`);
  const btns = h("div", "buttons");
  btns.append(
    button(t.progress != null ? "Resume" : "Play", icons.play, true, () => go(watchUrl(t.title, startOf(t).episode))),
    button("More info", icons.info, false, () => go(detailUrl(t.title))),
  );
  card.append(btns);
  el.append(card);

  let timer = 0;
  const open = () => {
    row.querySelectorAll(".tile.open").forEach((o) => o !== el && o.classList.remove("open"));
    el.classList.add("open");
    row.classList.add("open");
    const right = el.offsetLeft - track.scrollLeft + expandedWidth;
    const overflow = right - (track.clientWidth - inset);
    if (overflow > 0) track.scrollBy({ left: overflow, behavior: "smooth" });
  };
  const close = () => {
    clearTimeout(timer);
    el.classList.remove("open");
    row.classList.toggle("open", !!row.querySelector(".tile.open"));
  };
  const later = () => {
    clearTimeout(timer);
    timer = window.setTimeout(open, HOVER_DELAY);
  };
  el.addEventListener("mouseenter", later);
  el.addEventListener("mouseleave", close);
  el.addEventListener("focusin", later);
  el.addEventListener("focusout", (e) => {
    if (!el.contains(e.relatedTarget as Node)) close();
  });
  el.addEventListener("click", () => go(detailUrl(t.title)));
  el.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && e.target === el) go(detailUrl(t.title));
  });
  return el;
}

/** A row of tiles that scrolls sideways with arrow buttons. */
function rowEl(r: Row) {
  const row = h("section", "row", `<h2 class="row-label">${r.name}</h2>`);
  const viewport = h("div", "row-viewport");
  const track = h("div", "track");
  track.append(...r.tiles.map((t) => tileEl(t, row, track)));
  const arrow = (dir: -1 | 1) => {
    const b = h("button", `arrow ${dir < 0 ? "left" : "right"}`, `<svg viewBox="0 0 24 24"><path d="${dir < 0 ? "M15 4 L7 12 L15 20" : "M9 4 L17 12 L9 20"}" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/></svg>`);
    b.setAttribute("aria-label", dir < 0 ? "Scroll left" : "Scroll right");
    b.addEventListener("click", () => track.scrollBy({ left: dir * (track.clientWidth - inset * 2), behavior: "smooth" }));
    return b;
  };
  const left = arrow(-1);
  const right = arrow(1);
  const sync = () => {
    left.classList.toggle("hidden", track.scrollLeft <= 2);
    right.classList.toggle("hidden", track.scrollLeft + track.clientWidth >= track.scrollWidth - 2);
  };
  track.addEventListener("scroll", sync, { passive: true });
  requestAnimationFrame(sync);
  viewport.append(track, left, right);
  row.append(viewport);
  return row;
}

function rowsEl(rows: Row[], cls = "rows") {
  const el = h("div", cls);
  el.append(...rows.filter((r) => r.tiles.length).map(rowEl));
  return el;
}

function billboard(t: Title, kicker: string) {
  const el = h("section", "billboard", `<div class="bb-back"></div><div class="bb-fade"></div>
    <div class="bb-info"><div class="kicker">${kicker}</div><h1 class="bb-title">${t.name}</h1>
    <div class="meta">${metaHtml(t)}</div><p class="synopsis">${t.synopsis}</p></div>`);
  (el.querySelector(".bb-back") as HTMLElement).style.backgroundImage = backdropArt(t);
  const btns = h("div", "buttons");
  btns.append(
    button("Play", icons.play, true, () => go(watchUrl(t, startOf({ title: t }).episode))),
    button("More info", icons.info, false, () => go(detailUrl(t))),
  );
  el.querySelector(".bb-info")!.append(btns);
  return el;
}

/** A static grid for filtered lists and search results; tiles here do not expand. */
function grid(list: Title[], withSynopsis = false) {
  const el = h("div", "grid");
  el.append(...list.map((t) => {
    const a = h("a", "grid-tile", `<div class="grid-art"></div><div class="grid-name">${t.name}</div>
      <div class="grid-sub">${t.year} · ${t.kind === "movie" ? "Movie" : "Show"} · ${t.genres.join(", ")}</div>
      ${withSynopsis ? `<p class="grid-synopsis">${t.synopsis}</p>` : ""}`);
    a.href = detailUrl(t);
    (a.firstElementChild as HTMLElement).style.backgroundImage = tileArt(t);
    return a;
  }));
  return el;
}

function genreSelect(kind: Kind, current: string | null) {
  const genres = [...new Set((kind === "movie" ? movies : shows).flatMap((t) => t.genres))].sort();
  const sel = h("select", "select", `<option value="">All genres</option>${genres.map((g) => `<option${g === current ? " selected" : ""}>${g}</option>`).join("")}`);
  sel.setAttribute("aria-label", "Genre");
  sel.addEventListener("change", () => go(`#/${kind}s${sel.value ? `?genre=${encodeURIComponent(sel.value)}` : ""}`));
  return sel;
}

// ---------- navigation bar ----------

let searchInput: HTMLInputElement;

function buildNav() {
  if (!user) return;
  nav.className = "nav";
  nav.innerHTML = `<a class="brand" href="#/"><i>S</i>selfless</a>
    <nav class="links"><a href="#/" data-page="home">Home</a><a href="#/shows" data-page="shows">Shows</a><a href="#/movies" data-page="movies">Movies</a></nav>
    <label class="search">${icons.search}<input type="search" placeholder="Search titles and synopses" aria-label="Search" /></label>
    <div class="user-menu"><button class="user-btn" type="button" aria-haspopup="true"><i class="avatar"></i><span>${user.name}</span><svg viewBox="0 0 24 24"><path d="M6 9 L12 15 L18 9" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/></svg></button>
    <div class="user-pop" hidden><div class="user-pop-head"><i class="avatar"></i><div><b>${user.name}</b><small>Signed in</small></div></div><button type="button" data-act="signout">Sign out</button></div></div>`;
  nav.querySelectorAll<HTMLElement>(".avatar").forEach((a) => (a.style.backgroundImage = avatarArt(user!)));
  searchInput = nav.querySelector("input")!;
  searchInput.addEventListener("input", () => {
    const hash = `#/search?q=${encodeURIComponent(searchInput.value)}`;
    if (route().parts[0] === "search") {
      history.replaceState(null, "", hash);
      render();
    } else go(hash);
  });
  const pop = nav.querySelector<HTMLElement>(".user-pop")!;
  nav.querySelector(".user-btn")!.addEventListener("click", (e) => {
    e.stopPropagation();
    pop.hidden = !pop.hidden;
  });
  document.addEventListener("click", () => (pop.hidden = true));
  pop.querySelector("[data-act=signout]")!.addEventListener("click", () => {
    user = undefined;
    go("#/signin");
  });
}

function syncNav(page: string, q: URLSearchParams) {
  nav.hidden = !user || page === "signin" || page === "watch";
  nav.querySelectorAll<HTMLElement>(".links a").forEach((a) => a.classList.toggle("current", a.dataset.page === page));
  if (searchInput && document.activeElement !== searchInput) searchInput.value = page === "search" ? q.get("q") ?? "" : "";
}

addEventListener("scroll", () => nav.classList.toggle("solid", scrollY > 10), { passive: true });

// ---------- pages ----------

function home() {
  view.append(billboard(featuredFor("home"), `Featured ${featuredFor("home").kind === "movie" ? "movie" : "show"}`), rowsEl(rowsFor("home", user!)));
}

function list(kind: Kind, genre: string | null) {
  const label = kind === "movie" ? "Movies" : "Shows";
  const head = h("div", "page-head", `<h2>${label}</h2>`);
  head.append(genreSelect(kind, genre));
  if (!genre) {
    const bb = billboard(featuredFor(`${kind}s`), `Featured ${kind}`);
    bb.prepend(head);
    view.append(bb, rowsEl(rowsFor(`${kind}s`, user!)));
    return;
  }
  const found = (kind === "movie" ? movies : shows).filter((t) => t.genres.includes(genre));
  head.append(h("span", "count", `${found.length} ${found.length === 1 ? label.slice(0, -1).toLowerCase() : label.toLowerCase()}`));
  const page = h("div", "page");
  page.append(head, grid(found));
  view.append(page);
}

function search(query: string) {
  const q = query.trim().toLowerCase();
  const found = q ? titles.filter((t) => t.name.toLowerCase().includes(q) || t.synopsis.toLowerCase().includes(q)) : [];
  const page = h("div", "page");
  if (!q) page.append(h("div", "page-head", `<h2>Popular now</h2>`), grid([...titles].sort((a, b) => b.popularity - a.popularity).slice(0, 12)));
  else if (!found.length) page.append(h("div", "page-head", `<h2>No movies or shows match “${query}”</h2>`), h("p", "empty", "Try another word from a title or synopsis."));
  else page.append(h("div", "page-head", `<h2>Results for “${query}”</h2><span class="count">${found.length} ${found.length === 1 ? "title" : "titles"}</span>`), grid(found, true));
  view.append(page);
}

function hero(t: Title, inner: string) {
  const el = h("section", "hero", `<div class="bb-back"></div><div class="bb-fade"></div><div class="hero-info">${inner}</div>`);
  (el.querySelector(".bb-back") as HTMLElement).style.backgroundImage = backdropArt(t);
  return el;
}

function watchedButton() {
  let watched = false;
  const b = button("Mark as watched", icons.check, false, () => {
    watched = !watched;
    b.querySelector("span")!.textContent = watched ? "Watched" : "Mark as watched";
  });
  return b;
}

function movieDetail(t: Title) {
  const resume = resumeOf(user!, t);
  const el = hero(t, `<div class="kicker">Movie</div><h1>${t.name}</h1><div class="meta">${metaHtml(t)}</div>${resume ? resumeHtml(resume) : ""}<p class="synopsis full">${t.synopsis}</p>`);
  const btns = h("div", "buttons");
  if (resume) btns.append(button("Resume", icons.play, true, () => go(watchUrl(t))), button("Start over", icons.restart, false, () => go(watchUrl(t))));
  else btns.append(button("Play", icons.play, true, () => go(watchUrl(t))));
  btns.append(watchedButton());
  el.querySelector(".hero-info")!.append(btns);
  view.append(el, rowsEl([{ name: "More like this", tiles: moreLike(t) }], "rows detail-rows"));
}

function showDetail(t: Title, seasonParam: string | null) {
  const start = startOf({ title: t });
  const resume = resumeOf(user!, t);
  const nextEp = resume ? undefined : nextUp(user!).find((x) => x.title === t);
  const verb = resume ? "Resume" : nextEp ? "Play next" : "Play";
  const el = hero(t, `<div class="kicker">Show</div><h1>${t.name}</h1><div class="meta">${metaHtml(t)}</div>
    ${start.episode && (resume || nextEp) ? `<div class="ep-line"><b>${episodeLabel(start.episode)}</b> ${start.episode.name}</div>` : ""}${resume ? resumeHtml(resume) : ""}
    <p class="synopsis full">${t.synopsis}</p>`);
  const btns = h("div", "buttons");
  btns.append(button(`${verb} ${episodeLabel(start.episode!)}`, icons.play, true, () => go(watchUrl(t, start.episode))), watchedButton());
  el.querySelector(".hero-info")!.append(btns);

  let season = Number(seasonParam) || start.episode!.season;
  if (!t.seasons[season - 1]) season = 1;
  const section = h("section", "episodes-section");
  const head = h("div", "page-head", `<h2>Episodes</h2>`);
  const sel = h("select", "select", t.seasons.map((s) => `<option value="${s.number}"${s.number === season ? " selected" : ""}>Season ${s.number} (${s.episodes.length} episodes)</option>`).join(""));
  sel.setAttribute("aria-label", "Season");
  head.append(sel);
  const listEl = h("ol", "episodes");
  const fill = () => {
    listEl.replaceChildren(...t.seasons[season - 1].episodes.map((e) => {
      const isStart = start.episode === e && (resume || nextEp);
      const progress = isStart && resume?.progress != null ? `<div class="tile-progress"><i style="width:${resume.progress * 100}%"></i></div>` : "";
      const li = h("li");
      const a = h("a", `episode${isStart ? " current" : ""}`, `<span class="ep-num">${e.number}</span>
        <div class="ep-thumb"><span class="ep-play">${icons.play}</span>${progress}</div>
        <div class="ep-text"><div class="ep-title"><b>${e.name}</b>${isStart ? `<em>${resume ? "Resume" : "Next up"}</em>` : ""}<span>${e.runtime} min</span></div><p>${e.synopsis}</p></div>`);
      a.href = watchUrl(t, e);
      (a.querySelector(".ep-thumb") as HTMLElement).style.backgroundImage = tileArt(t, e.number + e.season * 20);
      li.append(a);
      return li;
    }));
  };
  sel.addEventListener("change", () => {
    season = Number(sel.value);
    history.replaceState(null, "", `#/show/${t.id}?season=${season}`);
    fill();
  });
  fill();
  section.append(head, listEl);
  view.append(el, section, rowsEl([{ name: "More like this", tiles: moreLike(t) }], "rows detail-rows"));
}

function watch(t: Title, q: URLSearchParams) {
  const e = t.seasons[Number(q.get("s")) - 1]?.episodes[Number(q.get("e")) - 1];
  const r = resumeOf(user!, t);
  const progress = r && (t.kind === "movie" || r.episode === e) ? r.progress ?? 0 : 0;
  const total = e ? e.runtime : t.runtime;
  const el = h("div", "player", `<button class="player-back" type="button"><svg viewBox="0 0 24 24"><path d="M15 4 L7 12 L15 20" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/></svg>Back</button>
    <div class="player-title"><h2>${t.name}</h2>${e ? `<div>${episodeLabel(e)} · ${e.name}</div>` : ""}</div>
    <div class="player-note">Prototype: no video.</div>
    <div class="player-bottom"><div class="bar"><i style="width:${progress * 100}%"></i></div><span>${Math.round(total * progress)}:00 / ${total}:00</span></div>`);
  el.style.backgroundImage = backdropArt(t);
  el.querySelector(".player-back")!.addEventListener("click", () => history.back());
  view.append(el);
}

function signIn() {
  const el = h("div", "signin");
  const mosaic = h("div", "signin-mosaic");
  mosaic.append(...[...titles, ...titles].slice(0, 40).map((t) => {
    const m = h("div");
    m.style.backgroundImage = tileArt(t);
    return m;
  }));
  const form = h("form", "signin-card", `<div class="brand"><i>S</i>selfless</div><h1>Sign in</h1>
    <label>Username<input name="username" autocomplete="username" autofocus /></label>
    <label>Password<input name="password" type="password" autocomplete="current-password" /></label>
    <p class="error" hidden>That username and password don't match.</p>
    <button class="btn primary wide" type="submit">Sign in</button>
    <p class="hint">Prototype: sign in as Maya, Theo, June or Sam with any password.</p>`);
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const data = new FormData(form);
    const found = users.find((u) => u.name.toLowerCase() === String(data.get("username")).trim().toLowerCase());
    if (!found || !data.get("password")) {
      form.querySelector<HTMLElement>(".error")!.hidden = false;
      return;
    }
    user = found;
    buildNav();
    go(afterSignIn);
  });
  el.append(mosaic, form);
  view.append(el);
}

// ---------- router ----------

function route() {
  const [path, query = ""] = location.hash.slice(1).split("?");
  return { parts: (path || "/").split("/").filter(Boolean), q: new URLSearchParams(query) };
}

function render() {
  const { parts, q } = route();
  const page = parts[0] ?? "home";
  if (!user && page !== "signin") {
    afterSignIn = location.hash || "#/";
    return location.replace("#/signin");
  }
  if (user && page === "signin") return location.replace("#/");
  syncNav(page, q);
  view.replaceChildren();
  const t = titles.find((x) => x.id === parts[1]);
  if (page === "home") home();
  else if (page === "signin") signIn();
  else if (page === "shows" || page === "movies") list(page === "shows" ? "show" : "movie", q.get("genre"));
  else if (page === "search") search(q.get("q") ?? "");
  else if (page === "movie" && t?.kind === "movie") movieDetail(t);
  else if (page === "show" && t?.kind === "show") showDetail(t, q.get("season"));
  else if (page === "watch" && t) watch(t, q);
  else view.append(h("div", "page", `<div class="page-head"><h2>Nothing here</h2></div><p class="empty"><a href="#/">Back to home</a></p>`));
  if (page !== "search") scrollTo(0, 0);
}

addEventListener("hashchange", render);
buildNav();
render();
