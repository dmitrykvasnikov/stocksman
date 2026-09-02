import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import App from "./App";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  invokeMock.mockReset();
});

describe("App", () => {
  it("presents the read-only product boundary in a browser preview", async () => {
    render(<App />);

    expect(
      screen.getByRole("heading", {
        name: /crypto charts, signals, and nothing that can move your money/i,
      }),
    ).toBeInTheDocument();
    expect(await screen.findByText("Browser preview")).toBeInTheDocument();
    expect(screen.getByText(/no accounts · no api keys · no trading/i)).toBeInTheDocument();
  });

  it("reports the supervised backend state in the desktop runtime", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    invokeMock.mockResolvedValue({
      application: "Stocksman",
      runtime: "Rust + Tokio",
      backend: {
        state: "ready",
        endpoint: "http://127.0.0.1:49152",
      },
    });

    render(<App />);

    expect(await screen.findByText("Backend ready")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("runtime_info");
  });
});
