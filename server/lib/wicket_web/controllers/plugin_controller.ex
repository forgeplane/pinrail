defmodule WicketWeb.PluginController do
  @moduledoc """
  Serves a plugin bundle at `/plugins/:type/:version/*path` from the version
  snapshot (or the live directory for a dev plugin), with the CSP that makes
  the sandbox real: no network at all. Scripts and styles only inline, from
  the bundle's own path, or the SDK under `/sdk/`; images and fonts only
  inline or from the bundle.

  The iframe loads these without `allow-same-origin`, so the document has an
  opaque origin and `'self'` would match nothing; the bundle path is spelled
  out with the host the browser used.
  """
  use WicketWeb, :controller

  alias Wicket.Types

  def show(conn, %{"type" => type, "version" => version, "path" => segments}) do
    with {:ok, version} <- parse_version(version),
         {:ok, plugin} <- Types.fetch(type, version),
         {:ok, path} <- safe_path(Types.bundle_dir(plugin), segments) do
      conn
      |> put_resp_header("content-security-policy", csp(conn, type, version))
      |> put_resp_header("cache-control", "no-cache")
      |> put_resp_header("x-content-type-options", "nosniff")
      |> put_resp_content_type(MIME.from_path(path))
      |> send_file(200, path)
    else
      _ -> send_resp(conn, 404, "not found")
    end
  end

  defp parse_version(s) do
    case Integer.parse(s) do
      {n, ""} when n > 0 -> {:ok, n}
      _ -> :error
    end
  end

  defp safe_path(dir, segments) do
    relative = Enum.join(segments, "/")

    with {:ok, safe} <- Path.safe_relative(relative, dir),
         path = Path.join(dir, safe),
         true <- File.regular?(path) do
      {:ok, path}
    else
      _ -> :error
    end
  end

  defp csp(conn, type, version) do
    origin = "#{conn.scheme}://#{conn.host}:#{conn.port}"
    bundle = "#{origin}/plugins/#{type}/#{version}/"

    Enum.join(
      [
        "default-src 'none'",
        "script-src 'unsafe-inline' #{bundle} #{origin}/sdk/",
        "style-src 'unsafe-inline' #{bundle} #{origin}/sdk/",
        # The app's own icon set is an image to the policy, because a CSS mask
        # is: same origin, same immutable path the SDK is served from.
        "img-src data: blob: #{bundle} #{origin}/sdk/",
        "font-src data: #{bundle}",
        "media-src data: blob: #{bundle}",
        "connect-src 'none'",
        "form-action 'none'",
        "base-uri 'none'",
        "frame-ancestors 'self'"
      ],
      "; "
    )
  end
end
