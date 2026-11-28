"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { errorMessage, postJson } from "@/components/auth/api";
import { outlineButton, primaryButton } from "@/components/ui";

export interface PricingPackage {
  id: string;
  name: string;
  maxLinkedAccounts: number;
  maxDevices: number;
  priceCents: number;
}

export interface PricingStrings {
  subscribe: string;
  loginToSubscribe: string;
  perMonth: string;
  free: string;
  linkedAccounts: string;
  devices: string;
  currentPlan: string;
  chooseProvider: string;
  checkoutRedirect: string;
}

const PROVIDER_LABELS: Record<string, string> = {
  stripe: "Stripe",
  paypal: "PayPal",
  polar: "Polar",
  freemius: "Freemius",
};

/** Public pricing grid with subscribe buttons (W6). */
export function PricingPlans({
  packages,
  providers,
  currentPackageId,
  signedIn,
  strings,
  errors,
  locale,
}: {
  packages: PricingPackage[];
  providers: string[];
  currentPackageId: string | null;
  signedIn: boolean;
  strings: PricingStrings;
  errors: Record<string, string>;
  locale: string;
}) {
  const router = useRouter();
  const [busyId, setBusyId] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [providerFor, setProviderFor] = useState<Record<string, string>>({});

  function formatPrice(cents: number): string {
    return new Intl.NumberFormat(locale, {
      style: "currency",
      currency: "USD",
    }).format(cents / 100);
  }

  async function subscribe(pkg: PricingPackage) {
    if (!signedIn) {
      router.push("/login?next=/pricing");
      return;
    }
    const provider = providerFor[pkg.id] ?? providers[0];
    if (!provider) return;
    setBusyId(pkg.id);
    setMessage(null);
    const result = await postJson("/api/billing/checkout", {
      packageId: pkg.id,
      provider,
    });
    if (!result.ok) {
      setMessage(errorMessage(errors, locale, result.data));
      setBusyId(null);
      return;
    }
    setMessage(strings.checkoutRedirect);
    const url = result.data.url;
    if (typeof url === "string") {
      window.location.href = url;
    } else {
      setBusyId(null);
    }
  }

  return (
    <div>
      <div className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {packages.map((pkg) => {
          const isCurrent = pkg.id === currentPackageId;
          // A package sells only when it has a price and at least one
          // provider is available for this buyer (otherwise no button).
          const forSale =
            pkg.name !== "Free" && pkg.priceCents > 0 && providers.length > 0;
          const provider = providerFor[pkg.id] ?? providers[0];
          return (
            <section
              key={pkg.id}
              className="rounded-2xl border border-border bg-card p-5"
            >
              <h2 className="text-lg font-semibold tracking-tight">
                {pkg.name}
              </h2>
              <p className="mt-1 text-2xl font-semibold tracking-tight">
                {pkg.priceCents === 0 ? (
                  strings.free
                ) : (
                  <>
                    {formatPrice(pkg.priceCents)}
                    <span className="text-sm font-normal text-muted-foreground">
                      {strings.perMonth}
                    </span>
                  </>
                )}
              </p>
              <ul className="mt-4 space-y-1 text-sm text-muted-foreground">
                <li>
                  {pkg.maxLinkedAccounts} {strings.linkedAccounts}
                </li>
                <li>
                  {pkg.maxDevices} {strings.devices}
                </li>
              </ul>
              {isCurrent ? (
                <p className="mt-5 text-sm font-medium">
                  {strings.currentPlan}
                </p>
              ) : forSale && signedIn && providers.length > 1 ? (
                <div className="mt-5">
                  <label
                    htmlFor={`provider-${pkg.id}`}
                    className="text-sm text-muted-foreground"
                  >
                    {strings.chooseProvider}
                  </label>
                  <select
                    id={`provider-${pkg.id}`}
                    value={provider}
                    onChange={(event) =>
                      setProviderFor((prev) => ({
                        ...prev,
                        [pkg.id]: event.target.value,
                      }))
                    }
                    className="mt-1 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  >
                    {providers.map((key) => (
                      <option key={key} value={key}>
                        {PROVIDER_LABELS[key] ?? key}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    className={`${primaryButton} mt-3 w-full`}
                    disabled={busyId === pkg.id}
                    onClick={() => subscribe(pkg)}
                  >
                    {strings.subscribe}
                  </button>
                </div>
              ) : forSale ? (
                <button
                  type="button"
                  className={`${primaryButton} mt-5 w-full`}
                  disabled={busyId === pkg.id}
                  onClick={() => subscribe(pkg)}
                >
                  {signedIn ? strings.subscribe : strings.loginToSubscribe}
                </button>
              ) : null}
            </section>
          );
        })}
      </div>
      {message ? (
        <p role="alert" className="mt-6 text-sm text-muted-foreground">
          {message}
        </p>
      ) : null}
      {!signedIn ? (
        <p className="mt-6 text-sm text-muted-foreground">
          <a href="/login" className={outlineButton}>
            {strings.loginToSubscribe}
          </a>
        </p>
      ) : null}
    </div>
  );
}
