import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import { render, screen } from "@testing-library/react";
import type { Repository } from "@reponyx/shared";
import Repositories from "./Repositories";

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

import { listAccounts, listRepositories } from "@reponyx/api-client";

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
});
