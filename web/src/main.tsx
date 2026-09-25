import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "react-router-dom";

import "./design/tokens.css";
import { router } from "./routes";
import { ThemeProvider, applyStoredTheme } from "./ui/theme";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // A console is read while something is happening. Stale data that
      // looks current is worse here than a visible refetch.
      staleTime: 2_000,
      refetchOnWindowFocus: true,
      retry: 1,
    },
  },
});

applyStoredTheme();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <RouterProvider router={router} />
      </ThemeProvider>
    </QueryClientProvider>
  </StrictMode>,
);
