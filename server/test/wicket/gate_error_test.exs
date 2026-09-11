defmodule Wicket.GateErrorTest do
  use ExUnit.Case, async: true

  alias Wicket.GateError

  test "reasons map to statuses and messages" do
    assert %GateError{reason: :not_found, message: "gate g_1 not found"} =
             e = GateError.not_found("g_1")

    assert GateError.status(e) == 404
    assert Plug.Exception.status(e) == 404
    assert GateError.status(GateError.not_pending("g_1")) == 409

    e = GateError.invalid([GateError.violation("/a", "is bad"), GateError.violation("", "nope")])
    assert GateError.status(e) == 422
    assert e.message == "validation failed:\n  /a: is bad\n  /: nope"
  end

  test "is raisable and serialisable" do
    assert_raise GateError, "gate g_2 not found", fn -> raise GateError.not_found("g_2") end

    assert GateError.to_map(GateError.invalid(GateError.violation("/x", "m"))) == %{
             "error" => "invalid",
             "message" => "validation failed:\n  /x: m",
             "violations" => [%{"path" => "/x", "message" => "m"}]
           }
  end
end
