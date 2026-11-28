import { formatDate } from "@reponyx/i18n";
import { countUsersAdmin, listUsersAdmin } from "@reponyx/database";
import { requireAdminPage } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { getRequestLocale, getWebTranslator } from "@/lib/i18n";
import { card, link, textInput } from "@/components/ui";

const PAGE_SIZE = 50;

/** Every user, newest first, with package and usage counts. */
export default async function AdminUsersPage(props: {
  searchParams: Promise<{ q?: string; offset?: string }>;
}) {
  const { q, offset: offsetParam } = await props.searchParams;
  const offset = Math.max(Number(offsetParam ?? 0) || 0, 0);
  const client = getDb();
  await requireAdminPage(client);
  const [users, total] = await Promise.all([
    listUsersAdmin(client, { q, limit: PAGE_SIZE, offset }),
    countUsersAdmin(client, q),
  ]);
  const locale = await getRequestLocale();
  const t = await getWebTranslator("admin");
  const tCommon = await getWebTranslator("common");
  const nextOffset = offset + PAGE_SIZE;
  return (
    <main id="main-content" className="mx-auto w-full max-w-5xl px-6 py-8">
      <h1 className="text-2xl font-semibold tracking-tight">{t("users")}</h1>
      <p className="mt-1 text-muted-foreground">
        {t("totalUsers")}: {total}
      </p>
      <form method="get" className="mt-6 flex gap-2">
        <input
          type="search"
          name="q"
          defaultValue={q ?? ""}
          placeholder={t("searchPlaceholder")}
          className={textInput}
        />
        <button type="submit" className={`${link} shrink-0 px-2`}>
          {t("search")}
        </button>
      </form>
      {users.length === 0 ? (
        <p className="mt-6 text-sm text-muted-foreground">{t("noUsers")}</p>
      ) : (
        <section className={`${card} mt-6 overflow-x-auto`}>
          <table className="w-full text-left text-sm">
            <thead>
              <tr className="border-b border-border">
                <th className="px-4 py-2">{t("email")}</th>
                <th className="px-4 py-2">{t("name")}</th>
                <th className="px-4 py-2">{t("package")}</th>
                <th className="px-4 py-2">{t("linked")}</th>
                <th className="px-4 py-2">{t("devices")}</th>
                <th className="px-4 py-2">{t("twoFactor")}</th>
                <th className="px-4 py-2">{t("joined")}</th>
              </tr>
            </thead>
            <tbody>
              {users.map((user) => (
                <tr key={user.id} className="border-b border-border">
                  <td className="px-4 py-2">
                    <a href={`/admin/users/${user.id}`} className={link}>
                      {user.email}
                    </a>
                  </td>
                  <td className="px-4 py-2">{user.name ?? "—"}</td>
                  <td className="px-4 py-2">{user.package_name ?? "—"}</td>
                  <td className="px-4 py-2">{Number(user.linked_count)}</td>
                  <td className="px-4 py-2">{Number(user.device_count)}</td>
                  <td className="px-4 py-2">{user.totp_secret ? "✓" : "—"}</td>
                  <td className="px-4 py-2">
                    {formatDate(locale, new Date(user.created_at))}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
      <div className="mt-4 flex gap-6">
        {offset > 0 ? (
          <a
            href={`/admin/users?${new URLSearchParams({
              ...(q ? { q } : {}),
              offset: String(Math.max(offset - PAGE_SIZE, 0)),
            }).toString()}`}
            className={link}
          >
            ← {tCommon("back")}
          </a>
        ) : null}
        {nextOffset < total ? (
          <a
            href={`/admin/users?${new URLSearchParams({
              ...(q ? { q } : {}),
              offset: String(nextOffset),
            }).toString()}`}
            className={link}
          >
            {t("next")} →
          </a>
        ) : null}
      </div>
    </main>
  );
}
