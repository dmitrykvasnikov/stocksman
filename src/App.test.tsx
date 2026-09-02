import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import App from "./App";

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
});
