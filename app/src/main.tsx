import React from "react";
import ReactDOM from "react-dom/client";

import App from "@/App";
import { ThemeProvider } from "@/components/theme-provider";
import { Toaster } from "@/components/ui/sonner";
import { applyStoredThemeEagerly } from "@/lib/theme";
import "@/index.css";

applyStoredThemeEagerly();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <App />
      <Toaster position="top-right" />
    </ThemeProvider>
  </React.StrictMode>,
);
