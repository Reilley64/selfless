import type { MoviePage } from "@selfless/api-client";
import { render, screen } from "@testing-library/react-native";
import { MovieHeader } from "./MovieHeader";

const image = (id: string, widths: number[]) => ({
  urls: widths.map((width) => ({ width, url: `/api/images/${id}/${width}/v1?expires=1793836800&signature=s` })),
});
const fanart = image("0190f3c4-7d8e-7f90-a1b2-c3d4e5f6a7b8", [480, 960, 1280, 1920]);
const logo = image("0190f3c4-8e9f-7a01-b2c3-d4e5f6a7b8c9", [300, 600]);

const amelie: MoviePage = {
  id: "0190f3c4-5b6a-7c8d-9e0f-1a2b3c4d5e6f",
  title: "Amélie",
  year: 2001,
  certification: "R",
  runtimeMinutes: 122,
  genres: ["Comedy", "Romance"],
  rating: 7.9,
  plot: "A shy waitress decides to change the lives of those around her.",
  studio: "UGC",
  originalTitle: "Le Fabuleux Destin d'Amélie Poulain",
  directors: ["Jean-Pierre Jeunet"],
  cast: [
    { name: "Audrey Tautou", role: "Amélie Poulain" },
    { name: "Mathieu Kassovitz", role: "Nino Quincampoix" },
    { name: "Rufus", role: null },
  ],
  images: { fanart, logo },
  progress: null,
  minutesLeft: null,
  watched: false,
  play: { resumePosition: null },
  moreLikeThis: [],
};

test("a movie with a logo shows the logo over the backdrop instead of the title text", async () => {
  await render(<MovieHeader movie={amelie} />);

  expect(screen.getByTestId("detail-backdrop")).toBeOnTheScreen();
  expect(screen.getByTestId("detail-logo")).toBeOnTheScreen();
  expect(screen.getByLabelText("Amélie")).toBeOnTheScreen();
  expect(screen.queryByText("Amélie")).toBeNull();
});

test("a movie without a logo shows its title as text", async () => {
  await render(<MovieHeader movie={{ ...amelie, images: { fanart, logo: null } }} />);

  expect(screen.queryByTestId("detail-logo")).toBeNull();
  expect(screen.getByText("Amélie")).toBeOnTheScreen();
});

test("the header lists the details, plot, director, cast, studio and original title", async () => {
  await render(<MovieHeader movie={amelie} />);

  expect(screen.getByText("2001")).toBeOnTheScreen();
  expect(screen.getByText("R")).toBeOnTheScreen();
  expect(screen.getByText("2h 2m")).toBeOnTheScreen();
  expect(screen.getByText("Comedy · Romance")).toBeOnTheScreen();
  expect(screen.getByText("★ 7.9")).toBeOnTheScreen();
  expect(screen.getByText("A shy waitress decides to change the lives of those around her.")).toBeOnTheScreen();
  expect(screen.getByText("Jean-Pierre Jeunet")).toBeOnTheScreen();
  expect(
    screen.getByText("Audrey Tautou (Amélie Poulain), Mathieu Kassovitz (Nino Quincampoix), Rufus"),
  ).toBeOnTheScreen();
  expect(screen.getByText("UGC")).toBeOnTheScreen();
  expect(screen.getByText("Le Fabuleux Destin d'Amélie Poulain")).toBeOnTheScreen();
});

test("the studio and original title are left out when they are the same as the title", async () => {
  await render(<MovieHeader movie={{ ...amelie, title: "Arrival", studio: "Arrival", originalTitle: "Arrival", images: { fanart, logo } }} />);

  expect(screen.queryByText("Studio")).toBeNull();
  expect(screen.queryByText("Original title")).toBeNull();
  expect(screen.queryByText("Arrival")).toBeNull();
});

test("a movie with a resume position shows how far the user got and the minutes left", async () => {
  await render(<MovieHeader movie={{ ...amelie, progress: 0.25, minutesLeft: 92, play: { resumePosition: 1830 } }} />);

  expect(screen.getByRole("progressbar")).toHaveAccessibilityValue({ now: 25 });
  expect(screen.getByText("92 min left")).toBeOnTheScreen();
});

test("a movie without a resume position shows no minutes left", async () => {
  await render(<MovieHeader movie={amelie} />);

  expect(screen.queryByRole("progressbar")).toBeNull();
  expect(screen.queryByText(/min left/)).toBeNull();
});
