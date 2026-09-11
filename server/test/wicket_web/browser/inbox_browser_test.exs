defmodule WicketWeb.Browser.InboxTest do
  @moduledoc """
  The inbox in a real browser: live inserts and the title badge.
  """
  use PhoenixTest.Playwright.Case, async: false

  import Wicket.GatesCase

  @moduletag :playwright

  setup do
    reset!()
  end

  test "a new gate appears without a reload and the title carries the count", %{conn: conn} do
    conn =
      conn
      |> visit("/")
      |> assert_has("#inbox-empty")
      |> assert_has("title", text: "Inbox · wicket")

    gate = create_gate!(%{title: "Fresh"})

    conn
    |> assert_has("#gate-#{gate.id}", text: "Fresh")
    |> assert_has("title", text: "(1) Inbox · wicket")
    |> click_link("Fresh")
    |> assert_path("/gates/#{gate.id}")
    |> assert_has("h1", text: "Fresh")
  end
end
