import type { ShowPage } from "@selfless/api-client";
import { render, screen } from "@testing-library/react-native";
import { ShowHeader, showPlayLabel } from "./ShowHeader";

const severance: ShowPage = {
  id: "0190f3c4-6c7d-7e8f-9a0b-1c2d3e4f5a6b",
  title: "Severance",
  year: 2022,
  certification: "TV-MA",
  status: "Continuing",
  network: "Apple TV+",
  seasonCount: 2,
  genres: ["Drama", "Mystery"],
  rating: 8.7,
  plot: "Office workers have their memories split.",
  cast: [
    { name: "Adam Scott", role: "Mark Scout" },
    { name: "Britt Lower", role: null },
  ],
  images: { fanart: null, logo: null },
  seasons: [],
  play: {
    verb: "play",
    resumePosition: null,
    season: { id: "0190f3c4-7d8e-7f90-a1b2-c3d4e5f6a7b8", number: 1 },
    episode: { id: "0190f3c4-ab1c-7d2e-8f3a-4b5c6d7e8f9a", number: 1, title: "Good News About Hell", mediaFileId: "0190f3c4-c2d3-7e4f-8a5b-6c7d8e9f0a1b" },
  },
  moreLikeThis: [],
};

test("the header lists the details, plot, network, status and cast", async () => {
  await render(<ShowHeader show={severance} />);

  expect(screen.getByText("Severance")).toBeOnTheScreen();
  expect(screen.getByText("2022")).toBeOnTheScreen();
  expect(screen.getByText("TV-MA")).toBeOnTheScreen();
  expect(screen.getByText("2 Seasons")).toBeOnTheScreen();
  expect(screen.getByText("Drama · Mystery")).toBeOnTheScreen();
  expect(screen.getByText("★ 8.7")).toBeOnTheScreen();
  expect(screen.getByText("Office workers have their memories split.")).toBeOnTheScreen();
  expect(screen.getByText("Apple TV+")).toBeOnTheScreen();
  expect(screen.getByText("Continuing")).toBeOnTheScreen();
  expect(screen.getByText("Adam Scott (Mark Scout), Britt Lower")).toBeOnTheScreen();
});

test("the Play label names the verb and the episode", () => {
  expect(showPlayLabel(severance.play)).toBe("Play S1:E1");
  expect(
    showPlayLabel({ ...severance.play, verb: "resume", season: { ...severance.play.season, number: 2 }, episode: { ...severance.play.episode, number: 4 } }),
  ).toBe("Resume S2:E4");
});
