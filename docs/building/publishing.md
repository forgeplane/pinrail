---
title: Publishing a plugin
description: "Publish a plugin as a GitHub release that anyone can install without a toolchain."
---

The best way to share a plugin is a GitHub release with the built bundle attached. People install it with one command, Pinrail downloads the bundle and serves it as it is, and nothing is built or run on their machine.

```sh
pinrail plugins install https://github.com/acme/ticket-triage/releases
```

## What a release needs

- **One `.zip` that is the bundle.** It holds `manifest.json`, the schemas and the view, either at the root of the archive or inside a single folder, the way most zip tools lay it out. If you attach several zips, name the bundle `pinrail-plugin.zip`.
- **A tag that matches the manifest's version**, with or without a leading `v`. A release tagged `v1.2.0` whose manifest says `1.1.0` is refused. A repository that releases several plugins can put the plugin's name before the version, as in `review-v1.2.0`. Pinrail then checks for updates only among the releases whose tags start with the same name.
- **The Pinrail it needs.** When your plugin relies on something a newer Pinrail adds, say so in the manifest, as `"pinrail": ">=0.2"`. An older app then refuses the plugin and tells the person which version to install, instead of loading a plugin that does not work.
- **No symbolic links.** Pinrail refuses to install a plugin whose files include one, because a link can point anywhere on the person's computer.
- **Only what the app serves.** Leave out sources, tests, fixtures, `node_modules` and tool configuration. Anything else in the zip is served with the view.

## Publish with GitHub Actions

The workflow below publishes a release when you push a tag. The SDK's `create` command writes it into every plugin it creates, as `.github/workflows/release.yml`. `pinrail plugins new` does not, so copy it into that file yourself. Then push a tag:

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

The workflow checks the version, runs the manifest's `build` command when there is one, zips the bundle as `<name>-<version>.zip`, and attaches it to a release of the same tag. It handles tags of the form `v<version>` only. A repository that puts the plugin's name before the version needs to change the tag pattern and the way the version is read from the tag.

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
      - name: Build when the manifest declares a build
        run: |
          command=$(node -e 'const m=require("./manifest.json"); process.stdout.write(m.build?.command ?? "")')
          if [ -n "$command" ]; then sh -c "$command"; fi
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

1. **Build**, if the manifest declares a `build` command.
2. **Zip** the folder without its sources, tests, fixtures, `node_modules` and dot-files.
3. **Attach** the zip to a GitHub release whose tag is the manifest's version.

## Choosing the version

Each version is on a line, a promise to every review already created with your plugin. From `1.0.0` on, the line is the major version. Before `1.0.0`, it is the major and the minor, so a plugin still finding its shape can make a breaking change from `0.3` to `0.4`. Pinrail keeps the latest release of each line that reviews use, and a review keeps rendering with the line it was created on.

| You changed | Release as |
|---|---|
| A fix in the view, or a new optional field | The same line: `1.2.0` → `1.3.0`, or `0.3.0` → `0.3.1`. Existing reviews pick it up. |
| A schema, or the view, in a way an old review would not survive | A new line: `1.3.0` → `2.0.0`, or `0.3.1` → `0.4.0`. Old reviews keep their line. |

## How people install and update it

| They install from | They get |
|---|---|
| `https://github.com/<owner>/<repo>/releases` | The repository's latest release. *Check for updates* compares its tag with what is installed. |
| `https://github.com/<owner>/<repo>/releases/tag/v1.2.0` | That release, pinned. A tag that is only a version is pinned, and update checks leave it where it is. |
| `https://github.com/<owner>/<repo>/releases/tag/review-v1.2.0` | That release, followed. A tag with the plugin's name before the version installs that release, and update checks then offer newer releases of the same plugin. |

A repository that releases several plugins has one latest release for all of them, so people should install each plugin from a tag URL rather than from `/releases`.

Publish the install command in your README, next to a screenshot of the view. That is usually all someone needs to decide whether to try it.
