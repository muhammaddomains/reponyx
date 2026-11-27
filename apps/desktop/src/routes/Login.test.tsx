import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import Login from "./Login";

vi.mock("@reponyx/api-client", () => ({
  webLogin: vi.fn(),
  webVerify2fa: vi.fn(),
  webOpenOAuth: vi.fn(),
  webOpenRegister: vi.fn(),
  webOpenUrl: vi.fn(),
}));

import {
  webLogin,
  webOpenOAuth,
  webOpenRegister,
  webOpenUrl,
  webVerify2fa,
} from "@reponyx/api-client";

const baseProps = {
  sessionExpired: false,
  initialPendingToken: null as string | null,
  registerCompleteToken: null as string | null,
  deepLinkError: null,
  onClearDeepLinkError: vi.fn(),
  onAuthenticated: vi.fn(),
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("Login page", () => {
  it("shows the login form with OAuth buttons and the register prompt", () => {
    render(<Login {...baseProps} />);
    expect(
      screen.getByText("Sign in with your RepoNyx account to sync your repositories."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Continue with Google" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Continue with GitHub" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Email")).toBeInTheDocument();
    expect(screen.getByLabelText("Password")).toBeInTheDocument();
    expect(screen.getByText("Don't have an account?")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Register on the web" }),
    ).toBeInTheDocument();
  });

  it("shows the 2FA step when the login stops there", async () => {
    vi.mocked(webLogin).mockResolvedValue({
      step: "2fa_required",
      pendingToken: "pending-token",
      user: null,
    });
    render(<Login {...baseProps} />);
    fireEvent.change(screen.getByLabelText("Email"), {
      target: { value: "user@example.com" },
    });
    fireEvent.change(screen.getByLabelText("Password"), {
      target: { value: "secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Log in" }));
    expect(await screen.findByText("Two-factor authentication")).toBeInTheDocument();
    expect(screen.getByLabelText("Authentication code")).toBeInTheDocument();
    expect(baseProps.onAuthenticated).not.toHaveBeenCalled();
  });

  it("completes the 2FA step and reports the login", async () => {
    vi.mocked(webVerify2fa).mockResolvedValue({
      step: "ok",
      pendingToken: null,
      user: {
        id: "019c0000-0000-7000-8000-000000000003",
        email: "user@example.com",
        name: "Test User",
        createdAt: "2026-11-27T00:00:00Z",
      },
    });
    render(<Login {...baseProps} initialPendingToken="pending-token" />);
    expect(screen.getByText("Two-factor authentication")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Authentication code"), {
      target: { value: "123456" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Verify" }));
    await vi.waitFor(() =>
      expect(vi.mocked(webVerify2fa)).toHaveBeenCalledWith(
        "pending-token",
        "123456",
      ),
    );
    expect(baseProps.onAuthenticated).toHaveBeenCalled();
  });

  it("shows the localized error when the password is wrong", async () => {
    vi.mocked(webLogin).mockRejectedValue({
      code: "invalid_credentials",
      message: "Email or password is incorrect",
      retryAt: null,
    });
    render(<Login {...baseProps} />);
    fireEvent.change(screen.getByLabelText("Email"), {
      target: { value: "user@example.com" },
    });
    fireEvent.change(screen.getByLabelText("Password"), {
      target: { value: "wrong" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Log in" }));
    expect(
      await screen.findByText("Email or password is incorrect"),
    ).toBeInTheDocument();
    expect(baseProps.onAuthenticated).not.toHaveBeenCalled();
  });

  it("shows the database-unavailable message when the database is down", async () => {
    vi.mocked(webLogin).mockRejectedValue({
      code: "DatabaseError",
      message: "database is unavailable",
      retryAt: null,
    });
    render(<Login {...baseProps} />);
    fireEvent.change(screen.getByLabelText("Email"), {
      target: { value: "user@example.com" },
    });
    fireEvent.change(screen.getByLabelText("Password"), {
      target: { value: "correct horse battery staple" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Log in" }));
    expect(
      await screen.findByText(
        "Database unavailable — close other RepoNyx windows and try again",
      ),
    ).toBeInTheDocument();
    expect(baseProps.onAuthenticated).not.toHaveBeenCalled();
  });

  it("shows the session-expired notice when the session died", () => {
    render(<Login {...baseProps} sessionExpired={true} />);
    expect(
      screen.getByText("Your session expired — please log in again"),
    ).toBeInTheDocument();
  });

  it("shows the register-complete card when the deep link carries a token", async () => {
    vi.mocked(webOpenUrl).mockResolvedValue(undefined);
    render(<Login {...baseProps} registerCompleteToken="reg-token" />);
    expect(screen.getByText("Finish your registration")).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "Complete registration" }),
    );
    await vi.waitFor(() =>
      expect(vi.mocked(webOpenUrl)).toHaveBeenCalledWith(
        "/register/complete?token=reg-token",
      ),
    );
  });

  it("opens the browser for OAuth and register", async () => {
    vi.mocked(webOpenOAuth).mockResolvedValue(undefined);
    vi.mocked(webOpenRegister).mockResolvedValue(undefined);
    render(<Login {...baseProps} />);
    fireEvent.click(
      screen.getByRole("button", { name: "Continue with GitHub" }),
    );
    await vi.waitFor(() =>
      expect(vi.mocked(webOpenOAuth)).toHaveBeenCalledWith("github"),
    );
    expect(
      await screen.findByText(
        "Continue in your browser — you'll come back here when you're done.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "Register on the web" }),
    );
    await vi.waitFor(() =>
      expect(vi.mocked(webOpenRegister)).toHaveBeenCalled(),
    );
  });

  it("shows a deep-link error from the OAuth callback", () => {
    render(
      <Login
        {...baseProps}
        deepLinkError={{ code: "oauth_no_account", retryAt: null }}
      />,
    );
    expect(
      screen.getByText("No account found — please register on the web first"),
    ).toBeInTheDocument();
  });

  it("clears a stale deep-link error when a new OAuth attempt starts", () => {
    render(
      <Login
        {...baseProps}
        deepLinkError={{ code: "oauth_failed", retryAt: null }}
      />,
    );
    expect(
      screen.getByText("Could not sign you in with that provider — try again"),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "Continue with Google" }),
    );
    expect(baseProps.onClearDeepLinkError).toHaveBeenCalled();
  });

  it("renders an unmapped web error code as the generic message", () => {
    render(
      <Login
        {...baseProps}
        deepLinkError={{ code: "internal_error", retryAt: null }}
      />,
    );
    expect(
      screen.getByText("Something went wrong — try again"),
    ).toBeInTheDocument();
  });
});
