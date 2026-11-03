import { useListShowsInfinite } from "@selfless/api-client";
import { useGridQuery } from "@selfless/ui";
import { GridPage, nextCursor } from "../../components/GridPage";

export default function Shows() {
  const { genre, sort } = useGridQuery();
  const query = useListShowsInfinite(
    { genre, sort },
    {
      query: { initialPageParam: undefined, getNextPageParam: nextCursor },
    },
  );
  return <GridPage title="Shows" kind="show" query={query} />;
}
