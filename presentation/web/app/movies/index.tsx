import { useListMoviesInfinite } from "@selfless/api-client";
import { GridPage, nextCursor } from "../../components/GridPage";

export default function Movies() {
  const query = useListMoviesInfinite(undefined, {
    query: { initialPageParam: undefined, getNextPageParam: nextCursor },
  });
  return <GridPage title="Movies" query={query} />;
}
