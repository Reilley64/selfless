import { fireEvent, renderRouter, screen } from "expo-router/testing-library";
import { PlayActions } from "./PlayActions";

const movieId = "0190f3c4-5b6a-7c8d-9e0f-1a2b3c4d5e6f";

async function renderPlayActions(resumePosition: number | null) {
  await renderRouter(
    {
      "movies/[movieId]": () => <PlayActions href={`/play/movies/${movieId}`} resumePosition={resumePosition} />,
      "play/movies/[movieId]": () => null,
    },
    { initialUrl: `/movies/${movieId}` },
  );
}

test("with a resume position, Resume plays from it and Start over plays from the start", async () => {
  await renderPlayActions(1534.2);

  expect(screen.queryByText("Play")).toBeNull();
  await fireEvent.press(screen.getByText("Start over"));

  expect(screen).toHavePathname(`/play/movies/${movieId}`);
  expect(screen).toHaveSearchParams({ movieId, from: "start" });
});

test("Resume opens the player without a start position", async () => {
  await renderPlayActions(1534.2);

  await fireEvent.press(screen.getByText("Resume"));

  expect(screen).toHavePathname(`/play/movies/${movieId}`);
  expect(screen).toHaveSearchParams({ movieId });
});

test("without a resume position, there is only Play", async () => {
  await renderPlayActions(null);

  expect(screen.queryByText("Resume")).toBeNull();
  expect(screen.queryByText("Start over")).toBeNull();
  await fireEvent.press(screen.getByText("Play"));

  expect(screen).toHavePathname(`/play/movies/${movieId}`);
});
