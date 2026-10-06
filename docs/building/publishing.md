---
title: Publishing a plugin
description: "Publish a plugin as a zip on a GitHub release that anyone can install without a toolchain."
---

The best way to share a plugin is a zip of its built bundle, attached to a GitHub release. People download the zip and install it with one command. Pinrail stores the bundle as it is, and nothing is built or run on their machine.

```sh
pinrail plugins install ~/Downloads/ticket-triage-1.2.0.zip
```

## What a release needs

- **One `.zip` that is the bundle.** It holds `manifest.json`, the schemas and the built view, either at the root of the archive or inside a single folder, the way most zip tools lay it out.
- **A tag that matches the manifest's version**, with or without a leading `v`, so that people can tell which version a release holds.
- **The Pinrail it needs.** When your plugin relies on something a newer Pinrail adds, say so in the manifest, as `"pinrail": ">=0.2"`. An older app then refuses the plugin and tells the person which version to install, instead of loading a plugin that does not work.
- **No symbolic links.** Pinrail refuses to install a plugin whose files include one, because a link can point anywhere on the person's computer.
- **Only what the app serves.** Leave out sources, tests, fixtures, `node_modules` and tool configuration. Anything else in the zip is served with the view.

## Publish with GitHub Actions

The workflow below publishes a release when you push a tag. `pinrail plugins new` does not write it, so copy it into `.github/workflows/release.yml` yourself. Then push a tag:

```sh
git tag v0.2.0
git push origin v0.2.0
```

```mermaid title="From a tag to an install"
flowchart LR
  T["push tag v0.2.0"] --> C{"version<br/>= tag?"}
  C -->|"no"| X["fail"]
  C -->|"yes"| B["build and zip"]
  B --> R["release v0.2.0"]
  R --> I(["pinrail plugins install"]):::you
```

The workflow checks the version, runs `npm ci && npm run build` when `package.json` has a `build` script, zips the bundle as `<name>-<version>.zip`, and attaches it to a release of the same tag. It handles tags of the form `v<version>` only. A repository that puts the plugin's name before the version needs to change the tag pattern and the way the version is read from the tag.

```yaml title=".github/workflows/release.yml"
name: release

on:
  push:
    tags: ["v*"]

# read-only, unless a job asks for more
permissions:
  contents: read

jobs:
  bundle:
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          persist-credentials: false
      - uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7.0.0
        with:
          node-version: 24
          # a release builds from nothing a cache could have changed
          package-manager-cache: false
      - name: Name and version
        run: |
          echo "PLUGIN=$(node -e 'process.stdout.write(require("./manifest.json").name)')" >> "$GITHUB_ENV"
          echo "VERSION=${GITHUB_REF_NAME#v}" >> "$GITHUB_ENV"
          declared=$(node -e 'process.stdout.write(require("./manifest.json").version)')
          test "$declared" = "${GITHUB_REF_NAME#v}" || { echo "manifest says $declared, tag says ${GITHUB_REF_NAME#v}"; exit 1; }
      - name: Build when the package has a build script
        run: |
          if [ -f package.json ] && node -e 'process.exit(require("./package.json").scripts?.build ? 0 : 1)'; then
            npm ci && npm run build
          fi
      - name: The bundle, and nothing else
        run: |
          zip -r "$PLUGIN-$VERSION.zip" . \
            -x "node_modules/*" "src/*" "tests/*" "test/*" "fixtures/*" ".*" "*/.*" "*.zip" \
               "package.json" "package-lock.json" "pnpm-lock.yaml" "yarn.lock" "bun.lockb" \
               "tsconfig*.json" "vite.config.*" "vitest.config.*" "playwright.config.*"
      - name: Release the bundle
        env:
          GH_TOKEN: ${{ github.token }}
        run: gh release create "$GITHUB_REF_NAME" "$PLUGIN-$VERSION.zip" --repo "$GITHUB_REPOSITORY" --verify-tag --title "$GITHUB_REF_NAME" --notes ""
```

:::tip[Bump the version first]
Raise `version` in `manifest.json`, commit, then tag. The workflow fails fast when the two disagree, before anything is published.
:::

## Without GitHub Actions

The recipe is three steps, in any CI or by hand:

1. **Build** the view, if the plugin has a build step.
2. **Zip** the folder without its sources, tests, fixtures, `node_modules` and dot-files.
3. **Publish** the zip, for example on a GitHub release whose tag is the manifest's version.

## Choosing the version

A pending review moves to a new release when it is next opened, if the release accepts its payload, and an ended review keeps the release it ended with. The version is a promise to the agents that use your plugin:

| You changed | Release as |
|---|---|
| A fix in the view, or a new optional field | A minor or a patch: `1.2.0` → `1.3.0`, or `0.3.0` → `0.3.1`. |
| A schema, in a way an earlier payload or decision would not pass | A new major: `1.3.0` → `2.0.0`. Before `1.0.0`, a new minor: `0.3.1` → `0.4.0`. |

Check a release against the previous one before you publish it:

```sh
pinrail plugins check . --since ../previous-release
```

The command lists what the new schemas no longer accept, such as a removed property, a newly required one or a changed `type`, and exits with `2` when the version does not announce the break. See [Versions](/docs/building/writing/#versions) for the full rule.

## How people install and upgrade it

People download the zip and install it from disk:

```sh
pinrail plugins install ~/Downloads/ticket-triage-1.2.0.zip
```

To upgrade, they download the zip of the new version and install it the same way. It replaces the installed version for new reviews and for the pending reviews it accepts. Reviews that have ended keep the version they ended with.

Publish the download link and the install command in your README, next to a screenshot of the view. That is usually all someone needs to decide whether to try it.
