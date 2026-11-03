import { nextEpisodeHref } from "./playHref";

test("the next episode plays from its start and carries the autoplay count and the prompt", () => {
  expect(nextEpisodeHref({ id: "s01e04" }, { autoplayed: 2, stillWatching: true })).toBe(
    "/play/episodes/s01e04?position=0&autoplayed=2&stillWatching=true",
  );
});
