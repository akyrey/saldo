import { QueryClientProvider } from "@tanstack/react-query";
import { createMemoryHistory, RouterProvider } from "@tanstack/react-router";
import { render } from "@testing-library/react";
import { setUnauthorizedListener } from "@/api/client";
import { createQueryClient } from "@/lib/query-client";
import { createAppRouter } from "@/router";

/** Renders the whole app at `path` with an in-memory history. */
export function renderApp(path: string) {
  const queryClient = createQueryClient();
  const router = createAppRouter(queryClient, createMemoryHistory({ initialEntries: [path] }));
  setUnauthorizedListener(() => {});
  render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { router, queryClient };
}
