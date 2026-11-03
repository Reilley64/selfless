import { useListShowsInfinite } from "@selfless/api-client";
import { GridPage, nextCursor } from "../../components/GridPage";

export default function Shows() {
  const query = useListShowsInfinite(undefined, {
    query: { initialPageParam: undefined, getNextPageParam: nextCursor },
  });
  return <GridPage title="Shows" query={query} />;
}
