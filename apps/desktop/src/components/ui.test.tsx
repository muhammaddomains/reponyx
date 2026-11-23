import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { Badge, Button, Card } from "@reponyx/ui";

describe("Button", () => {
  it("renders its children", () => {
    render(<Button>Save</Button>);
    expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();
  });

  it("is primary by default and danger on request", () => {
    const { rerender } = render(<Button>Save</Button>);
    expect(screen.getByRole("button")).toHaveClass("bg-zinc-900");
    rerender(<Button variant="danger">Delete</Button>);
    expect(screen.getByRole("button")).toHaveClass("bg-red-600");
  });

  it("honours disabled", () => {
    render(<Button disabled>Save</Button>);
    const button = screen.getByRole("button", { name: "Save" });
    expect(button).toBeDisabled();
    expect(button).toHaveClass("disabled:opacity-50");
  });
});

describe("Badge", () => {
  it("renders each variant with its colour", () => {
    const { rerender } = render(<Badge variant="success">Synced</Badge>);
    expect(screen.getByText("Synced")).toHaveClass("bg-emerald-100");
    rerender(<Badge variant="danger">Failed</Badge>);
    expect(screen.getByText("Failed")).toHaveClass("bg-red-100");
    rerender(<Badge variant="warning">Paused</Badge>);
    expect(screen.getByText("Paused")).toHaveClass("bg-amber-100");
  });
});

describe("Card", () => {
  it("renders children inside a bordered card", () => {
    render(
      <Card>
        <p>Inside</p>
      </Card>,
    );
    expect(screen.getByText("Inside")).toBeInTheDocument();
  });
});
