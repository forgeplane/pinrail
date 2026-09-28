// What a frame in this window may show: Pinrail's own server, and nothing
// else. A plugin's view can send its frame to any address, and the sandbox
// does not stop it; the window's policy does. The policy the app is built
// with names no port, since the server's port is a setting, and a page
// served by the dev server gets no built-in policy at all. So the window
// adds one that names the server it talks to. A page enforces every policy
// it has, so this one narrows the other, and needs no build to take effect.

import { serverUrl } from "../api/client";

export async function limitFramesToServer(): Promise<void> {
  const origin = new URL(await serverUrl()).origin;
  const policy = document.createElement("meta");
  policy.httpEquiv = "Content-Security-Policy";
  policy.content = `frame-src ${origin}`;
  document.head.prepend(policy);
}
