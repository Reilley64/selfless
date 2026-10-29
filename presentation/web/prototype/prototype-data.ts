// PROTOTYPE data: fake library and users, copied from presentation/tv/prototype. Throw away with the rest of this folder.

export type Kind = "movie" | "show";

export interface Episode {
  season: number;
  number: number;
  name: string;
  runtime: number;
  synopsis: string;
}

export interface Season {
  number: number;
  episodes: Episode[];
}

export interface Title {
  id: string;
  kind: Kind;
  name: string;
  year: number;
  rating: string;
  genres: string[];
  synopsis: string;
  runtime: number;
  seasons: Season[];
  hue: number;
  hue2: number;
  pattern: number;
  font: number;
  added: number;
  popularity: number;
}

export interface User {
  name: string;
  color: string;
  shape: number;
}

export interface Tile {
  title: Title;
  episode?: Episode;
  progress?: number;
}

export interface Row {
  name: string;
  tiles: Tile[];
}

export function rng(seed: number) {
  return () => {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

type Raw = [Kind, string, number, string, string[], string];

const raw: Raw[] = [
  ["movie", "The Glass Orchard", 2023, "PG-13", ["Drama", "Mystery"], "A botanist inherits a greenhouse where every tree grows from a stranger's memory, and one of them remembers her."],
  ["movie", "Low Tide at Marrow Point", 2021, "R", ["Thriller", "Mystery"], "When the sea pulls back further than it ever has, a fishing town finds a car on the seabed with the mayor's plates."],
  ["movie", "Signal Fire", 2024, "PG-13", ["Sci-fi", "Adventure"], "Two lighthouse keepers on a dead planet pick up a reply to a message they never sent."],
  ["movie", "Paper Moons", 2019, "PG", ["Comedy", "Family"], "A grumpy origami master is forced to teach a summer camp of eleven-year-olds who only want to fold rockets."],
  ["movie", "The Last Ferry to Ostra", 2022, "PG-13", ["Drama", "Romance"], "Strangers stuck on a stalled night ferry trade the stories they never told the people they were travelling to."],
  ["movie", "Copperhead Junction", 2018, "R", ["Western", "Crime"], "A retired railway detective rides out one more time to stop a heist he planned thirty years ago."],
  ["movie", "Northern Static", 2024, "R", ["Sci-fi", "Thriller"], "A radio operator at an Arctic station hears her own voice on a frequency nobody uses."],
  ["movie", "A Small Kind of Thunder", 2020, "PG", ["Comedy", "Drama"], "A drummer with stage fright joins her grandfather's marching band for the town's last parade."],
  ["movie", "Velvet Machine", 2023, "R", ["Sci-fi", "Crime"], "A jazz club's house robot starts solving the murders its owner keeps covering up."],
  ["movie", "The Cartographer's Daughter", 2017, "PG-13", ["Adventure", "Drama"], "Using her father's unfinished maps, a teenager sails to an island that only appears on Tuesdays."],
  ["movie", "Dust & Daylight", 2021, "PG-13", ["Western", "Drama"], "Two estranged sisters drive their mother's ashes across the desert in a car with no reverse gear."],
  ["movie", "Hollow Crown Hotel", 2022, "PG-13", ["Comedy", "Mystery"], "The night manager of a crumbling hotel has one weekend to find out which guest stole the crown jewels of a country that no longer exists."],
  ["movie", "Ninety Seconds", 2025, "R", ["Thriller", "Action"], "A bomb technician relives the same ninety seconds until she figures out who on her team set the timer."],
  ["movie", "The Orbit Below", 2025, "PG-13", ["Sci-fi", "Drama"], "Miners on a deep-sea station discover a second, upside-down sky beneath the ocean floor."],
  ["movie", "Kites Over Kestrel", 2016, "G", ["Family", "Animation"], "A kite that refuses to come down carries a shy boy and his dog across a windy valley."],
  ["movie", "Midnight Laundry", 2024, "PG-13", ["Comedy", "Romance"], "Two insomniacs keep meeting at the same 24-hour laundromat and keep failing to swap numbers."],
  ["show", "Harbor Lights", 2019, "TV-14", ["Drama", "Crime"], "A harbour master and a smuggler's daughter keep a port city running while both pretend not to know what the other does."],
  ["show", "The Quiet Division", 2022, "TV-MA", ["Crime", "Thriller"], "A police unit that only works cases nobody reported, because the victims never knew they were victims."],
  ["show", "Starfall Station", 2021, "TV-14", ["Sci-fi", "Adventure"], "The crew of a crumbling space station keep it alive one broken part at a time, with help from a very opinionated AI."],
  ["show", "Bakers of Bellwether", 2020, "TV-PG", ["Comedy", "Family"], "A chaotic family bakery tries to win the regional cake fair without burning down the shop."],
  ["show", "Fault Lines", 2023, "TV-MA", ["Drama", "Thriller"], "After an earthquake splits a wealthy suburb in two, its neighbours turn on each other."],
  ["show", "Ghosts of Gable Street", 2018, "TV-PG", ["Comedy", "Mystery"], "A young couple buy a terraced house and inherit six ghosts with very strong opinions about the renovation."],
  ["show", "Neon Parish", 2024, "TV-MA", ["Sci-fi", "Crime"], "In a rain-soaked future city, a priest who hears machine confessions investigates a murder no human could commit."],
  ["show", "The Long Weekend", 2022, "TV-14", ["Comedy", "Drama"], "Old university friends rent a lake house for one weekend a year and somehow it always goes wrong."],
  ["show", "Wild Atlas", 2020, "TV-G", ["Documentary", "Family"], "Each episode follows one animal across one remarkable year on a different continent."],
  ["show", "Ironwood", 2019, "TV-MA", ["Western", "Drama"], "A logging town's founding families fight over the last old-growth forest in the territory."],
  ["show", "Second Shift", 2023, "TV-14", ["Comedy"], "Night staff at a big-box store run a secret, very competitive after-hours league of their own invention."],
  ["show", "Tangent", 2025, "TV-14", ["Sci-fi", "Mystery"], "A maths teacher finds that solving certain equations moves her one street over into a slightly different town."],
  ["show", "Our Little Planet", 2021, "TV-G", ["Documentary"], "Tiny creatures, huge stakes: the hidden worlds of insects, moss and puddles filmed up close."],
  ["show", "Cold Case Kitchen", 2024, "TV-14", ["Crime", "Comedy"], "A former detective opens a diner where the regulars keep bringing her unsolved cases with their orders."],
];

const words = ["Pilot", "The Long Way Round", "Low Water", "Paper Trail", "Dead Reckoning", "Small Hours", "Open House", "Night Market", "Second Chances", "The Visitor", "High Ground", "Blackout", "Old Debts", "Homecoming", "Static", "Crossing", "The Offer", "Fault", "Wake", "First Light", "Echoes", "Ashes", "The Map", "Thaw"];
const epSynopses = [
  "An old favour comes back to collect, and nobody is ready to pay it.",
  "A storm cuts the town off, forcing rivals to share one roof for the night.",
  "A missing ledger points to someone everybody trusted.",
  "A celebration goes badly wrong when an uninvited guest shows up.",
  "Secrets surface when an inspector arrives a week early.",
  "A small lie snowballs into the week's biggest problem.",
  "Two old friends are forced onto opposite sides of a vote.",
  "A late-night discovery changes the plan for everyone.",
];

export const titles: Title[] = raw.map(([kind, name, year, rating, genres, synopsis], i) => {
  const r = rng(i * 97 + 13);
  const seasons: Season[] = [];
  if (kind === "show") {
    const count = 1 + Math.floor(r() * 4);
    for (let s = 1; s <= count; s++) {
      const episodes: Episode[] = [];
      const n = 6 + Math.floor(r() * 5);
      for (let e = 1; e <= n; e++) {
        episodes.push({
          season: s,
          number: e,
          name: e === 1 && s === 1 ? "Pilot" : words[Math.floor(r() * words.length)],
          runtime: 24 + Math.floor(r() * 36),
          synopsis: epSynopses[Math.floor(r() * epSynopses.length)],
        });
      }
      seasons.push({ number: s, episodes });
    }
  }
  return {
    id: `t${i}`,
    kind,
    name,
    year,
    rating,
    genres,
    synopsis,
    runtime: 88 + Math.floor(r() * 60),
    seasons,
    hue: Math.floor(r() * 360),
    hue2: Math.floor(r() * 360),
    pattern: i % 5,
    font: Math.floor(r() * 3),
    added: Math.floor(r() * 1000),
    popularity: Math.floor(r() * 1000),
  };
});

export const users: User[] = [
  { name: "Maya", color: "#d97706", shape: 0 },
  { name: "Theo", color: "#2563eb", shape: 1 },
  { name: "June", color: "#be185d", shape: 2 },
  { name: "Sam", color: "#059669", shape: 3 },
];

export const movies = titles.filter((t) => t.kind === "movie");
export const shows = titles.filter((t) => t.kind === "show");

const byGenre = (list: Title[], g: string) => list.filter((t) => t.genres.includes(g)).map((title) => ({ title }));
const byPopularity = (list: Title[]) => [...list].sort((a, b) => b.popularity - a.popularity);
const byAdded = (list: Title[]) => [...list].sort((a, b) => b.added - a.added);

export function continueWatching(user: User): Tile[] {
  const r = rng(users.indexOf(user) * 31 + 7);
  const picks = [...titles].sort(() => r() - 0.5).slice(0, 8);
  return picks.map((title) => {
    const progress = 0.15 + r() * 0.7;
    if (title.kind === "movie") return { title, progress };
    const season = title.seasons[Math.floor(r() * title.seasons.length)];
    return { title, progress, episode: season.episodes[Math.floor(r() * season.episodes.length)] };
  });
}

export function nextUp(user: User): Tile[] {
  const r = rng(users.indexOf(user) * 53 + 3);
  return [...shows].sort(() => r() - 0.5).slice(0, 7).map((title) => {
    const season = title.seasons[Math.floor(r() * title.seasons.length)];
    return { title, episode: season.episodes[Math.floor(r() * season.episodes.length)] };
  });
}

export function resumeOf(user: User, title: Title): Tile | undefined {
  return continueWatching(user).find((t) => t.title === title);
}

export type Page = "search" | "home" | "shows" | "movies";

export function rowsFor(page: Page, user: User): Row[] {
  if (page === "shows") {
    return [
      { name: "Popular now", tiles: byPopularity(shows).map((title) => ({ title })) },
      { name: "Next up", tiles: nextUp(user) },
      { name: "Recently added", tiles: byAdded(shows).map((title) => ({ title })) },
      { name: "Comedy", tiles: byGenre(shows, "Comedy") },
      { name: "Crime", tiles: byGenre(shows, "Crime") },
      { name: "Sci-fi", tiles: byGenre(shows, "Sci-fi") },
    ];
  }
  if (page === "movies") {
    return [
      { name: "Popular now", tiles: byPopularity(movies).map((title) => ({ title })) },
      { name: "Recently added", tiles: byAdded(movies).map((title) => ({ title })) },
      { name: "Sci-fi", tiles: byGenre(movies, "Sci-fi") },
      { name: "Comedy", tiles: byGenre(movies, "Comedy") },
      { name: "Drama", tiles: byGenre(movies, "Drama") },
    ];
  }
  return [
    { name: "Continue watching", tiles: continueWatching(user) },
    { name: "Next up", tiles: nextUp(user) },
    { name: "Recently added", tiles: byAdded(titles).slice(0, 12).map((title) => ({ title })) },
    { name: "Popular now", tiles: byPopularity(titles).slice(0, 12).map((title) => ({ title })) },
    { name: "Sci-fi", tiles: byGenre(titles, "Sci-fi") },
    { name: "Comedy", tiles: byGenre(titles, "Comedy") },
  ];
}

export function featuredFor(page: Page): Title {
  const list = page === "shows" ? shows : page === "movies" ? movies : titles;
  return byPopularity(list)[0];
}

export function moreLike(title: Title): Tile[] {
  return titles.filter((t) => t !== title && t.genres.some((g) => title.genres.includes(g))).map((t) => ({ title: t }));
}

export function metaLine(title: Title): string[] {
  const length = title.kind === "movie"
    ? `${Math.floor(title.runtime / 60)}h ${title.runtime % 60}m`
    : `${title.seasons.length} ${title.seasons.length === 1 ? "Season" : "Seasons"}`;
  return [String(title.year), title.rating, length, title.genres.join(" · ")];
}

export function episodeLabel(e: Episode) {
  return `S${e.season}:E${e.number}`;
}
