import { HashRouter, Navigate, Route, Routes } from "react-router";
import { Layout } from "./components/Layout";
import { useExternalLinks, useNativeRoutes } from "./lib/native";
import { History } from "./screens/History";
import { Inbox } from "./screens/Inbox";
import { ReviewScreen } from "./screens/ReviewScreen";
import { LiveProvider } from "./state/live";
import { SettingsProvider } from "./state/settings";
import { ToastProvider } from "./state/toasts";
import { TopBarProvider } from "./state/topbar";

function Shell() {
  useNativeRoutes();
  useExternalLinks();
  return (
    <Layout>
      <Routes>
        <Route path="/" element={<Inbox />} />
        <Route path="/reviews/:id" element={<ReviewScreen />} />
        <Route path="/history" element={<History />} />
        <Route path="/plugins" element={<Navigate to="/" replace state={{ settings: "plugins" }} />} />
      </Routes>
    </Layout>
  );
}

export function App() {
  return (
    <HashRouter>
      <LiveProvider>
        <TopBarProvider>
          <SettingsProvider>
            <ToastProvider>
              <Shell />
            </ToastProvider>
          </SettingsProvider>
        </TopBarProvider>
      </LiveProvider>
    </HashRouter>
  );
}
