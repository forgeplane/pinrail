import { HashRouter, Navigate, Route, Routes, useLocation } from "react-router";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Layout } from "./components/Layout";
import { Projects } from "./screens/Projects";
import { useExternalLinks, useNativeRoutes } from "./lib/native";
import { Inbox } from "./screens/Inbox";
import { ReviewScreen } from "./screens/ReviewScreen";
import { SessionScreen, Sessions } from "./screens/Sessions";
import { WorkItems } from "./screens/WorkItems";
import { LiveProvider } from "./state/live";
import { SettingsProvider, useSettings } from "./state/settings";
import { ToastProvider } from "./state/toasts";
import { TopBarProvider } from "./state/topbar";

/** The History page is now a tab of the inbox: old links open it, with their filters. */
function ToHistoryTab() {
  const { search } = useLocation();
  const params = new URLSearchParams(search);
  params.set("tab", "history");
  return <Navigate to={{ pathname: "/", search: `?${params}` }} replace />;
}

function Shell() {
  useNativeRoutes();
  useExternalLinks();
  const { pathname } = useLocation();
  return (
    <Layout>
      <ErrorBoundary resetKey={pathname}>
        <Routes>
          <Route path="/" element={<Inbox />} />
          <Route path="/reviews/:id" element={<ReviewScreen />} />
          <Route path="/history" element={<ToHistoryTab />} />
          <Route path="/sessions" element={<SessionsPage />} />
          <Route path="/sessions/:agent/:id" element={<SessionScreen />} />
          <Route path="/projects" element={<Projects />} />
          <Route path="/items" element={<WorkItems />} />
          <Route path="/plugins" element={<Navigate to="/" replace state={{ settings: "plugins" }} />} />
        </Routes>
      </ErrorBoundary>
    </Layout>
  );
}

/** The Sessions page, while sessions are on in Settings › Labs; the inbox
 *  otherwise, once the settings say so. */
function SessionsPage() {
  const { settings, loaded } = useSettings();
  if (loaded && !settings.labs.sessions) return <Navigate to="/" replace />;
  return <Sessions />;
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
