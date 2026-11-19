// The Filter and Display menus of a list page, at component level.
import { useState } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { cleanup, render } from "../testing/render";
import { ListMenuButtons, type Filter } from "./ListMenus";

afterEach(cleanup);

/** A list with a searchable project filter and a status filter. */
function Menus({ onProject = () => {} }: { onProject?: (value: string) => void }) {
  const [project, setProject] = useState("all");
  const filters: Filter[] = [
    {
      key: "project",
      label: "Project or repository",
      value: project,
      off: "all",
      searchable: true,
      choices: [
        { value: "all", label: "All" },
        { value: "acme/app", label: "acme/app" },
        { value: "acme/web", label: "acme/web" },
      ],
      onChange: (value) => {
        setProject(value);
        onProject(value);
      },
    },
  ];
  return <ListMenuButtons label="work items" filters={filters} toggles={[]} onReset={() => setProject("all")} />;
}

const row = () => page.getByRole("menuitem", { name: /^Project or repository/ });
const search = () => page.getByRole("textbox", { name: "Filter project or repository" });

describe("the Filter menu", () => {
  test("a submenu opened by a click has its search focused", async () => {
    await render(<Menus />, { bare: true });
    await userEvent.click(page.getByRole("button", { name: "Filter" }));
    await userEvent.click(row());
    await expect.element(search()).toHaveFocus();
  });

  test("a submenu the pointer opened keeps its search focused when its row is then clicked", async () => {
    await render(<Menus />, { bare: true });
    await userEvent.click(page.getByRole("button", { name: "Filter" }));
    await userEvent.hover(row());
    await expect.element(search()).toHaveFocus();
    await userEvent.click(row());
    await expect.element(search()).toHaveFocus();
  });

  test("what is typed narrows the choices, and Enter picks the first match", async () => {
    const onProject = vi.fn();
    await render(<Menus onProject={onProject} />, { bare: true });
    await userEvent.click(page.getByRole("button", { name: "Filter" }));
    await userEvent.click(row());
    await userEvent.type(search(), "web");
    await expect.element(page.getByRole("menuitemradio", { name: "acme/web" })).toBeVisible();
    await expect.element(page.getByRole("menuitemradio", { name: "acme/app" })).not.toBeInTheDocument();
    await userEvent.keyboard("{Enter}");
    expect(onProject).toHaveBeenCalledWith("acme/web");
  });

  test("Escape closes the menu and gives the focus back to its button", async () => {
    await render(<Menus />, { bare: true });
    const button = page.getByRole("button", { name: "Filter" });
    await userEvent.click(button);
    await expect.element(row()).toBeVisible();
    await userEvent.keyboard("{Escape}");
    await expect.element(row()).not.toBeInTheDocument();
    await expect.element(button).toHaveFocus();
  });
});
