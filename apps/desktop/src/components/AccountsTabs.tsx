import { NavLink } from "react-router";

const tabClass = ({ isActive }: { isActive: boolean }) =>
  `rounded-t-md px-4 py-2 text-sm ${
    isActive
      ? "border-b-2 border-zinc-900 font-medium text-zinc-900 dark:border-zinc-100 dark:text-zinc-100"
      : "text-zinc-500 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-100"
  }`;

/** The Accounts tab bar. Tab state comes from the route, not
 *  local state, so refresh and the back button work and each tab
 *  is deep-linkable. */
export default function AccountsTabs() {
  return (
    <nav
      aria-label="Accounts"
      className="flex gap-1 border-b border-zinc-200 dark:border-zinc-800"
    >
      <NavLink to="/accounts" end className={tabClass}>
        Connected accounts
      </NavLink>
      <NavLink to="/accounts/add" className={tabClass}>
        Add account
      </NavLink>
    </nav>
  );
}
