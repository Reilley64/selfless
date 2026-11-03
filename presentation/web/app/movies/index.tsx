import { useListMoviesInfinite } from "@selfless/api-client";
import { useGridQuery } from "@selfless/ui";
import { GridPage, nextCursor } from "../../components/GridPage";

export default function Movies() {
  const { genre, sort } = useGridQuery();
  const query = useListMoviesInfinite(
    { genre, sort },
    {
      query: { initialPageParam: undefined, getNextPageParam: nextCursor },
    },
  );
  return <GridPage title="Movies" kind="movie" query={query} />;
}
