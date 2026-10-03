# Security

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub: open the
repository's **Security** tab and choose **Report a vulnerability**. Do not
open a public issue.

Describe what you found, how to reproduce it, and what an attacker could do
with it. We will reply within a week, and we will credit you in the release
notes of the fix if you wish.

We fix vulnerabilities in the latest release only. Pinrail updates itself,
so older versions do not get separate patches.

## What we consider a vulnerability

Reviews often contain code and other private material that has not been
shared anywhere yet. Pinrail makes the following guarantees, and anything
that breaks one of them is a vulnerability:

- **Only programs on your machine can reach the server.** The server listens
  on `127.0.0.1`, only answers requests addressed to `127.0.0.1` or
  `localhost`, and ignores requests that change data unless they are sent as
  JSON. A website you visit must not be able to read, submit or decide a
  review.
- **A plugin's view cannot leave its sandbox.** A view can draw its content
  and exchange messages with the app, and nothing else. It has no network
  access and no storage, and it cannot see other reviews or any files except
  those attached to its own review. If a view can reach the network, the
  app's own pages or anything else on your machine, that is a vulnerability,
  whichever plugin it belongs to.
- **Attachments are only displayed inside a sandbox.** A plugin's view can
  ask for the contents of a file attached to its review and display it
  inside its sandbox. For example, the artifact plugin displays an attached
  HTML page. The app itself only lists attachments and saves them to disk.
  An attachment that can run code anywhere else is a vulnerability.
- **Updates are authentic.** The app only installs updates signed with the
  release key, and every release file has a build provenance attestation,
  which you can check with
  `gh attestation verify <file> --repo forgeplane/pinrail`.

## What we do not consider a vulnerability

The following behaviour is intentional. It is explained in
[What runs where](https://pinrail.dev/docs/concepts/trust/).

- **Plugins are not signed, and plugin authors are not vetted.** Installing a
  plugin runs nothing: Pinrail copies its static files from a folder or a
  zip. Building a plugin yourself, for one that has a build step, runs that
  build on your machine with your permissions.
- **Any program on your machine can use the server.** Programs running under
  your user account can submit and read reviews, just as the `pinrail`
  command does.
- **Pinrail does not control your agents.** Whether an agent asks for a
  review, and whether it follows your decision, depends on its instructions
  and the tool that runs it.

Report problems in third-party plugins to their authors, unless the problem
lets the plugin escape its sandbox.
