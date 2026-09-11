# Browser tests are tagged :playwright and excluded unless asked for explicitly
# (`mix test.browser`, or `mix test --include playwright`). Only then is the
# endpoint switched to serve requests and the Playwright supervisor started.
browser? = :playwright in List.wrap(ExUnit.configuration()[:include])

if browser? do
  endpoint_config = Application.get_env(:wicket, WicketWeb.Endpoint)
  Application.put_env(:wicket, WicketWeb.Endpoint, Keyword.put(endpoint_config, :server, true))
  :ok = Supervisor.terminate_child(Wicket.Supervisor, WicketWeb.Endpoint)
  {:ok, _} = Supervisor.restart_child(Wicket.Supervisor, WicketWeb.Endpoint)
  {:ok, _} = PhoenixTest.Playwright.Supervisor.start_link()
  Application.put_env(:phoenix_test, :base_url, WicketWeb.Endpoint.url())
end

ExUnit.start(exclude: [:playwright])
