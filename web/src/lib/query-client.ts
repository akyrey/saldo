// TanStack Query keeps a cache of server responses in the browser, keyed by
// request (the "query key"). It is close to Laravel's `Cache::remember()`
// plus invalidation: a page asks for `["accounts"]`, gets the cached value at
// once, and the library refetches in the background when it may be stale.
// After a write we call `invalidateQueries` with a key prefix, the same as
// forgetting a cache tag.
import { QueryClient } from "@tanstack/react-query";
import { ApiError } from "@/api/errors";

const MAX_RETRIES = 2;

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        // Retrying a 4xx never helps; retry only network and server errors.
        retry: (failures, error) =>
          failures < MAX_RETRIES &&
          !(error instanceof ApiError && error.status >= 400 && error.status < 500),
      },
    },
  });
}
