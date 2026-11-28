"use client";

import { useState } from "react";
import { errorMessage } from "@/components/auth/api";
import {
  alertError,
  alertSuccess,
  card,
  fieldLabel,
  fieldWrapper,
  outlineButton,
  primaryButton,
  textInput,
} from "@/components/ui";

export interface BillingProviderInfo {
  provider: string;
  isActive: boolean;
  countries: string[] | null;
  configured: boolean;
}

export interface BillingPackageInfo {
  id: string;
  name: string;
  priceCents: number;
}

export interface BillingPriceInfo {
  package_id: string;
  provider: string;
  provider_price_id: string;
}

export interface BillingStrings {
  provider: string;
  configured: string;
  notConfigured: string;
  active: string;
  countries: string;
  worldwide: string;
  priceId: string;
  setPrice: string;
  priceSaved: string;
  providerUpdated: string;
  noPrice: string;
  save: string;
}

const PROVIDER_LABELS: Record<string, string> = {
  stripe: "Stripe",
  paypal: "PayPal",
  polar: "Polar",
  freemius: "Freemius",
  gumroad: "Gumroad",
};

/** Selling providers — Gumroad is registered only, never at checkout. */
const SELLING_PROVIDERS = ["stripe", "paypal", "polar", "freemius"];

async function requestJson(
  path: string,
  method: string,
  body: unknown
): Promise<{ ok: boolean; data: Record<string, unknown> }> {
  const res = await fetch(path, {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const data = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  return { ok: res.ok, data };
}

/** Admin billing: provider toggles + country scopes + price ids (W6). */
export function BillingManager({
  initialProviders,
  packages,
  initialPrices,
  strings,
  errors,
  locale,
}: {
  initialProviders: BillingProviderInfo[];
  packages: BillingPackageInfo[];
  initialPrices: BillingPriceInfo[];
  strings: BillingStrings;
  errors: Record<string, string>;
  locale: string;
}) {
  const [providers, setProviders] =
    useState<BillingProviderInfo[]>(initialProviders);
  const [countries, setCountries] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      initialProviders.map((p) => [
        p.provider,
        p.countries === null ? "" : p.countries.join(", "),
      ])
    )
  );
  const [prices, setPrices] = useState<Record<string, string>>(() => {
    const map: Record<string, string> = {};
    for (const price of initialPrices) {
      map[`${price.package_id}:${price.provider}`] = price.provider_price_id;
    }
    return map;
  });
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [messageOk, setMessageOk] = useState(false);

  async function saveProvider(provider: BillingProviderInfo) {
    setBusy(`provider:${provider.provider}`);
    setMessage(null);
    const rawCountries = countries[provider.provider] ?? "";
    const parsed =
      rawCountries.trim() === ""
        ? null
        : rawCountries
            .split(",")
            .map((code) => code.trim().toUpperCase())
            .filter((code) => code !== "");
    const result = await requestJson("/api/admin/billing/providers", "PATCH", {
      provider: provider.provider,
      isActive: provider.isActive,
      countries: parsed,
    });
    setBusy(null);
    if (!result.ok) {
      setMessage(errorMessage(errors, locale, result.data));
      setMessageOk(false);
      return;
    }
    const updated = result.data.provider as {
      provider: string;
      is_active: boolean;
      countries: string[] | null;
    };
    setProviders((prev) =>
      prev.map((p) =>
        p.provider === updated.provider
          ? {
              ...p,
              isActive: updated.is_active,
              countries: updated.countries,
            }
          : p
      )
    );
    setMessage(strings.providerUpdated);
    setMessageOk(true);
  }

  async function savePrice(packageId: string, provider: string) {
    const key = `${packageId}:${provider}`;
    const priceId = (prices[key] ?? "").trim();
    if (priceId === "") return;
    setBusy(`price:${key}`);
    setMessage(null);
    const result = await requestJson(
      "/api/admin/billing/package-prices",
      "PUT",
      { packageId, provider, providerPriceId: priceId }
    );
    setBusy(null);
    if (!result.ok) {
      setMessage(errorMessage(errors, locale, result.data));
      setMessageOk(false);
      return;
    }
    setMessage(strings.priceSaved);
    setMessageOk(true);
  }

  return (
    <div className="mt-8 space-y-8">
      <div className="grid gap-4 sm:grid-cols-2">
        {providers.map((provider) => (
          <section key={provider.provider} className={`${card} p-5`}>
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-semibold tracking-tight">
                {PROVIDER_LABELS[provider.provider] ?? provider.provider}
              </h2>
              <span className="text-sm text-muted-foreground">
                {provider.configured
                  ? strings.configured
                  : strings.notConfigured}
              </span>
            </div>
            <label className="mt-4 flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={provider.isActive}
                disabled={!provider.configured}
                onChange={(event) =>
                  setProviders((prev) =>
                    prev.map((p) =>
                      p.provider === provider.provider
                        ? { ...p, isActive: event.target.checked }
                        : p
                    )
                  )
                }
              />
              {strings.active}
            </label>
            <div className={`${fieldWrapper} mt-4`}>
              <label
                htmlFor={`countries-${provider.provider}`}
                className={fieldLabel}
              >
                {strings.countries} ({strings.worldwide} = {'""'})
              </label>
              <input
                id={`countries-${provider.provider}`}
                type="text"
                value={countries[provider.provider] ?? ""}
                onChange={(event) =>
                  setCountries((prev) => ({
                    ...prev,
                    [provider.provider]: event.target.value,
                  }))
                }
                placeholder="US, GB"
                className={textInput}
              />
            </div>
            <button
              type="button"
              className={`${primaryButton} mt-4`}
              disabled={busy === `provider:${provider.provider}`}
              onClick={() => saveProvider(provider)}
            >
              {strings.save}
            </button>
          </section>
        ))}
      </div>

      <section className={`${card} p-5`}>
        <h2 className="text-lg font-semibold tracking-tight">
          {strings.priceId}
        </h2>
        <div className="mt-4 space-y-4">
          {packages
            .filter((pkg) => pkg.name !== "Free")
            .map((pkg) =>
              SELLING_PROVIDERS.map((provider) => {
                const key = `${pkg.id}:${provider}`;
                const value = prices[key] ?? "";
                return (
                  <div key={key} className="flex flex-wrap items-end gap-3">
                    <div className={`${fieldWrapper} min-w-40 flex-1`}>
                      <label htmlFor={`price-${key}`} className={fieldLabel}>
                        {pkg.name} — {PROVIDER_LABELS[provider] ?? provider}
                      </label>
                      <input
                        id={`price-${key}`}
                        type="text"
                        value={value}
                        onChange={(event) =>
                          setPrices((prev) => ({
                            ...prev,
                            [key]: event.target.value,
                          }))
                        }
                        placeholder={strings.noPrice}
                        className={textInput}
                      />
                    </div>
                    <button
                      type="button"
                      className={outlineButton}
                      disabled={busy === `price:${key}` || value.trim() === ""}
                      onClick={() => savePrice(pkg.id, provider)}
                    >
                      {strings.setPrice}
                    </button>
                  </div>
                );
              })
            )}
        </div>
      </section>

      {message ? (
        <p role="alert" className={messageOk ? alertSuccess : alertError}>
          {message}
        </p>
      ) : null}
    </div>
  );
}
