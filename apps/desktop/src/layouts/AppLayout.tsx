import {
  Activity,
  Bot,
  CalendarClock,
  FolderGit2,
  HardDrive,
  LayoutDashboard,
  Moon,
  RefreshCw,
  Settings,
  Sun,
  Users,
} from "lucide-react";
import { NavLink, Outlet } from "react-router";
import { useTheme } from "../hooks/useTheme";

const NAV = [
  { to: "/", label: "Overview", icon: LayoutDashboard, end: true },
  { to: "/accounts", label: "Accounts", icon: Users },
  { to: "/repositories", label: "Repositories", icon: FolderGit2 },
  { to: "/synchronization", label: "Synchronization", icon: RefreshCw },
  { to: "/schedules", label: "Schedules", icon: CalendarClock },
  { to: "/agent", label: "Agent", icon: Bot },
  { to: "/activity", label: "Activity", icon: Activity },
  { to: "/storage", label: "Storage", icon: HardDrive },
  { to: "/settings", label: "Settings", icon: Settings },
];

export default function AppLayout() {
  const { theme, toggleTheme } = useTheme();

  return (
    <div className="flex h-screen bg-zinc-50 text-zinc-900 dark:bg-zinc-950 dark:text-zinc-100">
      <aside className="flex w-60 shrink-0 flex-col border-r border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
        <div className="flex items-center gap-2 px-5 py-4">
          <FolderGit2 className="h-6 w-6 text-zinc-900 dark:text-zinc-100" />
          <div>
            <div className="text-base font-semibold leading-tight">RepoNyx</div>
            <div className="text-xs text-zinc-500">Your Git. Always in Sync.</div>
          </div>
        </div>
        <nav className="flex-1 space-y-1 px-3 py-2">
          {NAV.map(({ to, label, icon: Icon, end }) => (
            <NavLink
              key={to}
              to={to}
              end={end}
              className={({ isActive }) =>
                `flex items-center gap-2.5 rounded-md px-3 py-2 text-sm ${
                  isActive
                    ? "bg-zinc-900 font-medium text-white dark:bg-zinc-100 dark:text-zinc-900"
                    : "text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
                }`
              }
            >
              <Icon className="h-4 w-4" />
              {label}
            </NavLink>
          ))}
        </nav>
        <div className="border-t border-zinc-200 px-3 py-3 dark:border-zinc-800">
          <button
            onClick={toggleTheme}
            className="flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
          >
            {theme === "dark" ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
            {theme === "dark" ? "Light mode" : "Dark mode"}
          </button>
        </div>
      </aside>
      <main className="min-w-0 flex-1 overflow-y-auto">
        <Outlet />
      </main>
    </div>
  );
}
