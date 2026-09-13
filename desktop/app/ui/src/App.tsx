import { HashRouter, Route, Routes } from "react-router";
import { Layout } from "./components/Layout";
import { History } from "./screens/History";
import { Inbox } from "./screens/Inbox";
import { Plugins } from "./screens/Plugins";
import { ReviewScreen } from "./screens/ReviewScreen";
import { LiveProvider } from "./state/live";

export function App() {
  return (
    <HashRouter>
      <LiveProvider>
        <Layout>
          <Routes>
            <Route path="/" element={<Inbox />} />
            <Route path="/reviews/:id" element={<ReviewScreen />} />
            <Route path="/history" element={<History />} />
            <Route path="/plugins" element={<Plugins />} />
          </Routes>
        </Layout>
      </LiveProvider>
    </HashRouter>
  );
}
