import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { MemoryRouter } from "react-router";

import { App } from "@/App";

/** Mounts the whole shell at one route, the way a browser and Playwright see it. */
export function renderApp(route: string) {
  const user = userEvent.setup();
  const result = render(
    <MemoryRouter initialEntries={[route]}>
      <App />
    </MemoryRouter>,
  );
  return { user, ...result };
}

/** Mounts one component, for views with no route of their own. */
export function renderComponent(element: ReactElement) {
  const user = userEvent.setup();
  return { user, ...render(element) };
}
