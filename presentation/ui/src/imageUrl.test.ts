import { type Artwork, provideSession } from "@selfless/api-client";
import { imageUrl } from "./imageUrl";

const fanart: Artwork = {
  urls: [480, 960, 1280, 1920].map((width) => ({
    width,
    url: `/api/images/0190f3c4-7d8e-7f90-a1b2-c3d4e5f6a7b8/${width}/v1?expires=1793836800&signature=s${width}`,
  })),
};

test("an image URL is the smallest width that covers the drawn size, on the server the app talks to", () => {
  provideSession({ serverUrl: () => "http://192.168.1.20:8080", token: () => undefined, ended: () => {} });

  expect(imageUrl(fanart, 600)).toBe(
    "http://192.168.1.20:8080/api/images/0190f3c4-7d8e-7f90-a1b2-c3d4e5f6a7b8/960/v1?expires=1793836800&signature=s960",
  );
  expect(imageUrl(fanart, 480)).toContain("/480/v1");
  expect(imageUrl(fanart, 2400)).toContain("/1920/v1");
});
