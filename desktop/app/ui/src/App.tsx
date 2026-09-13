import { HashRouter, Route, Routes } from "react-router";
import { Layout } from "./components/Layout";
import { useExternalLinks, useNativeRoutes } from "./lib/native";
import { History } from "./screens/History";
import { Inbox } from "./screens/Inbox";
import { Plugins } from "./screens/Plugins";
import { ReviewScreen } from "./screens/ReviewScreen";
import { LiveProvider } from "./state/live";

function Shell() {
  useNativeRoutes();
  useExternalLinks();
  return (
    <Layout>
      <Routes>
        <Route path="/" element={<Inbox />} />
        <Route path="/reviews/:id" element={<ReviewScreen />} />
        <Route path="/history" element={<History />} />
        <Route path="/plugins" element={<Plugins />} />
      </Routes>
    </Layout>
  );
}

export function App() {
  return (
    <HashRouter>
      <LiveProvider>
        <Shell />
      </LiveProvider>
    </HashRouter>
  );
}
