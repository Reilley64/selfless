import type { EpisodePage } from "@selfless/api-client";
import { render, screen } from "@testing-library/react-native";
import { EpisodeHeader } from "./EpisodeHeader";

const image = (id: string, widths: number[]) => ({
  urls: widths.map((width) => ({ width, url: `/api/images/${id}/${width}/v1?expires=1793836800&signature=s` })),
});

const halfLoop: EpisodePage = {
  id: "0190f3c4-ab1c-7d2e-8f3a-4b5c6d7e8f9a",
  title: "Half Loop",
  seasonNumber: 1,
  episodeNumber: 2,
  airDate: "2022-02-18",
  runtimeMinutes: 57,
  rating: 8.1,
  plot: "Mark takes Helly to the break room.",
  thumb: image("0190f3c4-7d8e-7f90-a1b2-c3d4e5f6a7b8", [480, 960]),
  show: {
    id: "0190f3c4-6c7d-7e8f-9a0b-1c2d3e4f5a6b",
    title: "Severance",
    logo: image("0190f3c4-8e9f-7a01-b2c3-d4e5f6a7b8c9", [300, 600]),
  },
  audioLanguages: ["eng", "jpn"],
  subtitleLanguages: [
    { language: "eng", hearingImpaired: false, forced: true },
    { language: "eng", hearingImpaired: false, forced: false },
    { language: "en", hearingImpaired: false, forced: false },
    { language: "fr", hearingImpaired: true, forced: false },
  ],
  progress: null,
  watched: false,
  play: { resumePosition: null },
};

test("the header shows the thumb, the show's logo, S:E and title, air date, runtime, rating and plot", async () => {
  await render(<EpisodeHeader episode={halfLoop} />);

  expect(screen.getByTestId("detail-backdrop")).toBeOnTheScreen();
  expect(screen.getByTestId("detail-logo")).toBeOnTheScreen();
  expect(screen.getByLabelText("Severance")).toBeOnTheScreen();
  expect(screen.getByText("S1:E2 · Half Loop")).toBeOnTheScreen();
  expect(screen.getByText("Feb 18, 2022")).toBeOnTheScreen();
  expect(screen.getByText("57m")).toBeOnTheScreen();
  expect(screen.getByText("★ 8.1")).toBeOnTheScreen();
  expect(screen.getByText("Mark takes Helly to the break room.")).toBeOnTheScreen();
});

test("the languages are named once each, with the subtitle flags", async () => {
  await render(<EpisodeHeader episode={halfLoop} />);

  expect(screen.getByText("English, Japanese")).toBeOnTheScreen();
  expect(screen.getByText("English (Forced), English, French (SDH)")).toBeOnTheScreen();
});

test("an episode without languages lists none", async () => {
  await render(<EpisodeHeader episode={{ ...halfLoop, audioLanguages: [], subtitleLanguages: [] }} />);

  expect(screen.queryByText("Audio")).toBeNull();
  expect(screen.queryByText("Subtitles")).toBeNull();
});
