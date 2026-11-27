# @reponyx/i18n — shared translations (web + desktop)

One shared package for both products, same shape as sendcms's
`packages/i18n`: `locales/{en,ur}/{web,desktop}/{domain}.json` — language
first, then product, one file per feature/domain, never one big file per
locale. Framework-free: `Intl` + JSON only, `{name}` interpolation,
`Intl.PluralRules`, locale-aware date/number/currency helpers.

## Languages

- `en` — source of truth, complete.
- `ur` — Urdu script (اردو), complete. RTL done properly: `dir="rtl"`,
  logical CSS, sidebars/menus flip side, numbers stay Latin digits.

A new language = a new `locales/<code>/` folder + one line in
`src/locales.ts`. Only `complete` locales appear in the switcher.

## Rules

- Missing key renders `[missing: key]` — never a crash, never silent.
- `pnpm --filter @reponyx/i18n validate` checks en/ur key parity and
  fails CI on a mismatch. Run it after adding or renaming a key.
- Server errors are localized by the client: the API returns code +
  data only, and the client renders from `errors.json`.
- The web statically imports the JSON files it needs and registers
  them via `registerTranslations`; `loadLocaleDomain` is the lazy
  path for clients that don't statically import.
