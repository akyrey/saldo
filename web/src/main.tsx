import { QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { setUnauthorizedListener } from "@/api/client";
import { createQueryClient } from "@/lib/query-client";
import { createAppRouter } from "@/router";
import "./index.css";

const queryClient = createQueryClient();
const router = createAppRouter(queryClient);

// A 401 anywhere (session expired, logged out in another tab) sends the
// person to the login page, then back to where they were.
// Without a known session the route guard handles it (setup or login).
setUnauthorizedListener(() => {
  if (queryClient.getQueryData(["me"]) === undefined) return;
  queryClient.removeQueries({ queryKey: ["me"] });
  const path = router.state.location.pathname;
  if (path === "/login" || path === "/setup") return;
  void router.navigate({ to: "/login", search: { redirect: router.state.location.href } });
});

const root = document.getElementById("root");
if (!root) throw new Error("index.html has no #root element");

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
);
