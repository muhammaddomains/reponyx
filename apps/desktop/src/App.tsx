import { HashRouter, Route, Routes } from "react-router";
import AppLayout from "./layouts/AppLayout";
import Accounts from "./routes/Accounts";
import Agent from "./routes/Agent";
import Overview from "./routes/Overview";
import Placeholder from "./routes/Placeholder";
import Repositories from "./routes/Repositories";
import Schedules from "./routes/Schedules";
import Synchronization from "./routes/Synchronization";

export default function App() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<AppLayout />}>
          <Route index element={<Overview />} />
          <Route path="accounts" element={<Accounts />} />
          <Route path="agent" element={<Agent />} />
          <Route path="repositories" element={<Repositories />} />
          <Route path="synchronization" element={<Synchronization />} />
          <Route path="schedules" element={<Schedules />} />
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
