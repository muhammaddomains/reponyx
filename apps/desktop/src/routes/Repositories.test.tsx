import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import {
  act,
  render,
  screen,
  fireEvent,
  waitFor,
  within,
} from "@testing-library/react";
import type { Repository } from "@reponyx/shared";
import Repositories from "./Repositories";

const { progressListeners } = vi.hoisted(() => ({
  progressListeners: [] as Array<(event: { payload: unknown }) => void>,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    (_eventName: string, handler: (event: { payload: unknown }) => void) => {
      progressListeners.push(handler);
      return Promise.resolve(() => {});
    },
  ),
}));

vi.mock("@reponyx/api-client", () => ({
  agentEnqueue: vi.fn(),
  deleteRepository: vi.fn(),
  discoverRepositories: vi.fn(),
  ensureAgentRunning: vi.fn(),
  listAccounts: vi.fn(),
  listRepositories: vi.fn(),
  openRepositoryInBrowser: vi.fn(),
  openRepositoryLocation: vi.fn(),
  updateRepository: vi.fn(),
}));

import {
  agentEnqueue,
  deleteRepository,
  ensureAgentRunning,
  listAccounts,
  listRepositories,
  updateRepository,
} from "@reponyx/api-client";

function emitProgress(payload: unknown) {
  act(() => {
    for (const listener of progressListeners) {
      listener({ payload });
    }
  });
}

function repo(fullName: string): Repository {
  const [owner, name] = fullName.split("/");
  return {
    id: `id-${name}`,
    accountId: "account-1",
    providerRepositoryId: `provider-${name}`,
    owner,
    name,
    fullName,
    visibility: "public",
    defaultBranch: "main",
    cloneUrl: `https://github.com/${fullName}.git`,
    archiveStatus: "synced",
    syncEnabled: true,
    syncMode: "mirror",
    localPath: null,
    remoteUpdatedAt: null,
    lastSyncedAt: null,
    createdAt: "2026-11-23T00:00:00Z",
    updatedAt: "2026-11-23T00:00:00Z",
  };
}

const offsetHeightDescriptor = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  "offsetHeight",
);
const clientHeightDescriptor = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  "clientHeight",
);
const clientWidthDescriptor = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  "clientWidth",
);

beforeAll(() => {
  // jsdom has no layout engine, so every element measures 0 and the
  // virtualizer collapses to an empty list. A real browser measures
  // real sizes (the virtualizer reads `offsetHeight` for rows and
  // `clientHeight`/`clientWidth` for the scroll container); here
  // rows report 190px and the container 640px so the virtual
  // window renders. Scoped to this file only.
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    get: () => 190,
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get: () => 640,
  });
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get: () => 800,
  });
});

afterAll(() => {
  if (offsetHeightDescriptor) {
    Object.defineProperty(
      HTMLElement.prototype,
      "offsetHeight",
      offsetHeightDescriptor,
    );
  }
  if (clientHeightDescriptor) {
    Object.defineProperty(
      HTMLElement.prototype,
      "clientHeight",
      clientHeightDescriptor,
    );
  }
  if (clientWidthDescriptor) {
    Object.defineProperty(
      HTMLElement.prototype,
      "clientWidth",
      clientWidthDescriptor,
    );
  }
});

beforeEach(() => {
  vi.clearAllMocks();
  progressListeners.length = 0;
  vi.mocked(listAccounts).mockResolvedValue([]);
});

describe("Repositories page (virtualized list)", () => {
  it("renders the loaded rows", async () => {
    vi.mocked(listRepositories).mockResolvedValue({
      items: [
        repo("muhammaddomains/reponyx"),
        repo("muhammaddomains/other"),
        repo("muhammaddomains/third"),
      ],
      total: 3,
    });
    render(<Repositories />);
    expect(
      await screen.findByText("muhammaddomains/reponyx"),
    ).toBeInTheDocument();
    expect(screen.getByText("muhammaddomains/other")).toBeInTheDocument();
    expect(screen.getByText("muhammaddomains/third")).toBeInTheDocument();
    expect(screen.getByText("All repositories (3)")).toBeInTheDocument();
  });

  it("shows an empty state when nothing matches", async () => {
    vi.mocked(listRepositories).mockResolvedValue({ items: [], total: 0 });
    render(<Repositories />);
    expect(
      await screen.findByText(/No repositories match/),
    ).toBeInTheDocument();
  });

  it("offers Show more while more rows remain server-side", async () => {
    vi.mocked(listRepositories).mockResolvedValue({
      items: [repo("muhammaddomains/reponyx")],
      total: 250,
    });
    render(<Repositories />);
    expect(
      await screen.findByRole("button", { name: /Show more \(249 remaining\)/ }),
    ).toBeInTheDocument();
  });

  it("removes all selected repositories with one confirm", async () => {
    vi.mocked(listRepositories).mockResolvedValue({
      items: [
        repo("muhammaddomains/reponyx"),
        repo("muhammaddomains/other"),
        repo("muhammaddomains/third"),
      ],
      total: 3,
    });
    render(<Repositories />);
    await screen.findByText("muhammaddomains/reponyx");
    fireEvent.click(screen.getByLabelText("Select muhammaddomains/other"));
    fireEvent.click(screen.getByLabelText("Select muhammaddomains/third"));
    fireEvent.click(
      screen.getByRole("button", { name: "Remove selected" }),
    );
    // The in-app dialog (not window.confirm) gates the delete.
    const dialog = screen.getByRole("alertdialog", {
      name: "Remove selected repositories",
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Remove selected" }),
    );
    expect(
      await screen.findByText("Removed 2 repositories. Local files were kept."),
    ).toBeInTheDocument();
    expect(vi.mocked(deleteRepository).mock.calls).toEqual([
      ["id-other"],
      ["id-third"],
    ]);
    // The selection is cleared, so the bulk toolbar goes away.
    expect(
      screen.queryByRole("button", { name: "Remove selected" }),
    ).not.toBeInTheDocument();
  });

  it("removes nothing when the bulk remove confirm is dismissed", async () => {
    vi.mocked(listRepositories).mockResolvedValue({
      items: [repo("muhammaddomains/reponyx")],
      total: 1,
    });
    render(<Repositories />);
    await screen.findByText("muhammaddomains/reponyx");
    fireEvent.click(screen.getByLabelText("Select muhammaddomains/reponyx"));
    fireEvent.click(
      screen.getByRole("button", { name: "Remove selected" }),
    );
    const dialog = screen.getByRole("alertdialog", {
      name: "Remove selected repositories",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(vi.mocked(deleteRepository)).not.toHaveBeenCalled();
    expect(
      screen.queryByText(/Removed 1 repositories/),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("alertdialog"),
    ).not.toBeInTheDocument();
  });

  it("removes one repository after confirming the row dialog", async () => {
    vi.mocked(listRepositories).mockResolvedValue({
      items: [repo("muhammaddomains/reponyx")],
      total: 1,
    });
    render(<Repositories />);
    await screen.findByText("muhammaddomains/reponyx");
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    const dialog = screen.getByRole("alertdialog", {
      name: "Remove repository",
    });
    expect(dialog).toHaveTextContent(
      "Remove muhammaddomains/reponyx from RepoNyx?",
    );
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    expect(
      await screen.findByText(
        "Removed muhammaddomains/reponyx. Local files were kept.",
      ),
    ).toBeInTheDocument();
    expect(vi.mocked(deleteRepository).mock.calls).toEqual([["id-reponyx"]]);
  });

  it("removes nothing when the row remove dialog is cancelled", async () => {
    vi.mocked(listRepositories).mockResolvedValue({
      items: [repo("muhammaddomains/reponyx")],
      total: 1,
    });
    render(<Repositories />);
    await screen.findByText("muhammaddomains/reponyx");
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    const dialog = screen.getByRole("alertdialog", {
      name: "Remove repository",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(vi.mocked(deleteRepository)).not.toHaveBeenCalled();
    expect(
      screen.queryByRole("alertdialog"),
    ).not.toBeInTheDocument();
    expect(screen.getByText("muhammaddomains/reponyx")).toBeInTheDocument();
  });

  it("mode notice names the mode just picked, not the previous one", async () => {
    const initial = repo("muhammaddomains/reponyx");
    const updated: Repository = { ...initial, syncMode: "working" };
    vi.mocked(listRepositories)
      .mockResolvedValueOnce({ items: [initial], total: 1 })
      .mockResolvedValue({ items: [updated], total: 1 });
    vi.mocked(updateRepository).mockResolvedValue(updated);
    render(<Repositories />);
    const select = await screen.findByLabelText(
      "Sync mode for muhammaddomains/reponyx",
    );
    fireEvent.change(select, { target: { value: "working" } });
    expect(
      await screen.findByText(
        "muhammaddomains/reponyx mode set to working.",
      ),
    ).toBeInTheDocument();
  });

  it("clears the queued notice and refreshes the row when the sync completes", async () => {
    const pending: Repository = {
      ...repo("DashingMZA/sendcms"),
      archiveStatus: "pending",
      localPath: null,
      lastSyncedAt: null,
    };
    const synced: Repository = {
      ...pending,
      archiveStatus: "synced",
      localPath:
        "D:\\github\\zb\\articles\\working-clones\\DashingMZA\\sendcms",
      lastSyncedAt: "2026-11-28T03:56:14Z",
    };
    vi.mocked(listRepositories)
      .mockResolvedValueOnce({ items: [pending], total: 1 })
      .mockResolvedValueOnce({ items: [pending], total: 1 })
      .mockResolvedValue({ items: [synced], total: 1 });
    vi.mocked(ensureAgentRunning).mockResolvedValue({
      reachable: true,
      status: null,
      message: "ok",
    });
    vi.mocked(agentEnqueue).mockResolvedValue(
      "080c7a5b-0000-4000-8000-000000000000",
    );
    render(<Repositories />);
    await screen.findByText("DashingMZA/sendcms");
    expect(screen.getByRole("button", { name: "Open folder" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Sync now" }));
    expect(
      await screen.findByText(/Sync queued for DashingMZA\/sendcms/),
    ).toBeInTheDocument();
    // The post-enqueue refresh still shows the pending row.
    expect(screen.getByRole("button", { name: "Open folder" })).toBeDisabled();
    emitProgress({
      type: "job_completed",
      job_id: "080c7a5b-0000-4000-8000-000000000000",
      repository_id: "id-sendcms",
      duration_ms: 1000,
    });
    await waitFor(() => {
      expect(screen.queryByText(/Sync queued for/)).not.toBeInTheDocument();
    });
    expect(screen.getByRole("button", { name: "Open folder" })).toBeEnabled();
  });

  it("shows the failure and clears the queued notice when the sync fails", async () => {
    const pending: Repository = {
      ...repo("DashingMZA/sendcms"),
      archiveStatus: "pending",
      localPath: null,
      lastSyncedAt: null,
    };
    vi.mocked(listRepositories).mockResolvedValue({
      items: [pending],
      total: 1,
    });
    vi.mocked(ensureAgentRunning).mockResolvedValue({
      reachable: true,
      status: null,
      message: "ok",
    });
    vi.mocked(agentEnqueue).mockResolvedValue(
      "080c7a5b-0000-4000-8000-000000000000",
    );
    render(<Repositories />);
    await screen.findByText("DashingMZA/sendcms");
    fireEvent.click(screen.getByRole("button", { name: "Sync now" }));
    expect(
      await screen.findByText(/Sync queued for DashingMZA\/sendcms/),
    ).toBeInTheDocument();
    emitProgress({
      type: "job_failed",
      job_id: "080c7a5b-0000-4000-8000-000000000000",
      repository_id: "id-sendcms",
      error_code: "InvalidInput",
      message: "GitHub App sync requires a paired device identifier",
      will_retry: false,
      next_attempt: null,
    });
    expect(
      await screen.findByText(
        "DashingMZA/sendcms: GitHub App sync requires a paired device identifier",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Sync queued for/)).not.toBeInTheDocument();
  });
});
