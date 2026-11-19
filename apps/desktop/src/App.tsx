import { HashRouter, Route, Routes } from "react-router";
import AppLayout from "./layouts/AppLayout";
import Accounts from "./routes/Accounts";
import Overview from "./routes/Overview";
import Placeholder from "./routes/Placeholder";

export default function App() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<AppLayout />}>
          <Route index element={<Overview />} />
          <Route path="accounts" element={<Accounts />} />
          <Route
            path="repositories"
            element={<Placeholder title="Repositories" phase={8} />}
          />
          <Route
            path="synchronization"
            element={<Placeholder title="Synchronization" phase={6} />}
          />
          <Route
            path="schedules"
            element={<Placeholder title="Schedules" phase={7} />}
          />
          <Route
            path="activity"
            element={<Placeholder title="Activity" phase={8} />}
          />
          <Route
            path="storage"
            element={<Placeholder title="Storage" phase={9} />}
          />
          <Route
            path="settings"
            element={<Placeholder title="Settings" phase={8} />}
          />
        </Route>
      </Routes>
    </HashRouter>
  );
}
