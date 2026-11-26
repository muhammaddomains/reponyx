# Accounts page restructure — plan (2026-11-26, Phase 6)

## The problem (user's words)

One Accounts page holds everything: the connected-accounts list,
the PAT form, and the GitHub App / broker section. With 10–50
accounts connected, the page becomes one very long scroll — the
list a user checks daily is buried under forms they use once.

## Recommendation: one sidebar entry, two tabs

Keep a single **Accounts** entry in the sidebar. Inside it, two
top-level tabs:

| Tab | Route | What lives there |
| --- | --- | --- |
| **Connected accounts** (default) | `/accounts` | The accounts list, with search + filters |
| **Add account** | `/accounts/add` | PAT form + GitHub App (broker) section |

Tab state comes from the **route**, not local state, so refresh
and the back button work, and each tab is deep-linkable.

### Why tabs, not two sidebar pages

- One concept, one place. "Where do I add" and "where do I see"
  are two views of the same thing; splitting them across two
  sidebar entries makes users hunt, and the sidebar already has
  9 items.
- The default view is what users see 99% of the time (their
  accounts). The add flow is one click away, not one scroll away.
- Two pages with nested tabs (PAT page + Auth page, each with
  tabs) is four surfaces for three concepts — over-engineered.
  PAT and Broker are two *methods* of doing one thing (adding an
  account), not two destinations, so they sit as two stacked
  cards inside the Add tab, not as sub-tabs.

### Connected accounts tab

- Summary line: counts by status, e.g. "3 connected · 1 revoked".
- Search input (filters by username, client-side, instant).
- Kind filter: All / GitHub App / PAT. Status filter: All /
  Connected / Revoked.
- Each row keeps what it has now: name + kind badge + status
  badge, Validate, Remove, and the per-kind actions (Replace PAT /
  Reconnect for PAT rows).
- Empty states: "No accounts yet — add your first account"
  (button → Add tab); "No accounts match this search".
- Scale: 50 light cards render fine with no virtualization. The
  search + filters are the scale mechanism. If accounts ever
  reach the hundreds, reuse the virtualization the Repositories
  page already has — noted as future work, not built now.

### Add account tab

Two stacked cards, in the order a new user meets them:

1. **Add a fine-grained PAT** — the existing form, unchanged.
2. **GitHub App (Auth Broker)** — broker status line, the setup
   form (App id + key file) when the broker has no config,
   Install GitHub App + Refresh, and the "Pair this device" flow.
   (Simpler since the 2026-11-26 auto-token change: no token
   field anywhere.)

The broker installations list does **not** move here as a
separate list — each installation already appears as an account
row in the Connected tab. The Install button stays with the
broker setup, where it belongs.

## Code changes (frontend only — no backend work)

- `apps/desktop/src/routes/Accounts.tsx` (~700 lines today)
  splits into:
  - `routes/Accounts.tsx` — tab shell + Connected list + filters.
  - `routes/AddAccount.tsx` — PAT card + GitHub App card.
  - `components/BrokerSetup.tsx` — broker setup form + Install +
    pairing (extracted, since its state is self-contained).
- Router: add the `/accounts/add` route next to `/accounts`.
- Tests: existing Accounts tests split by tab; new tests for tab
  switching via routes, search filter, kind/status filters, and
  both empty states. No new backend commands, no DB or settings
  changes, no migration.
- Compatibility: `/accounts` keeps working (default tab); no
  user data moves.

## Effort

Small, frontend-only: about one session (split + filters +
tests). No backend risk; the only CI surface is the frontend
suite. Implementation waits on the user's approval of this plan.
