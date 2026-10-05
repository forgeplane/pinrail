# The Linux app in Docker

This folder runs the Pinrail desktop app on Linux in a container, on a
virtual display that you see and use in a browser. It is for trying the
Linux build from macOS or any other machine with Docker.

## Start it

From the repository's root:

```sh
mise run linux-desktop
```

The first run compiles the app and the `pinrail` command and takes
several minutes. Later runs reuse the build and start much faster. When
the log says `the app is at …`, open:

<http://localhost:6080/vnc.html?autoconnect=1&resize=scale>

The display has a panel at the bottom with the app's tray icon, and
notifications appear in the top-right corner.

To stop it, press <kbd>Ctrl</kbd>+<kbd>C</kbd> in the terminal where it
runs, or run `docker rm -f pinrail-linux`.

## Send a review

Run the `pinrail` command inside the container, as the user the app
runs as:

```sh
docker exec -u pinrail pinrail-linux pinrail submit list --sample
```

`--sample` sends the review a plugin ships to show what it looks like.
The `list` and `feedback` plugins come with the app; install one first with
`pinrail plugins install list` if the setup did not. To send your own
review instead:

```sh
docker exec -u pinrail pinrail-linux pinrail submit list --title "Try it" \
  --data '{"groups":[{"title":"Checks","items":[{"id":1,"title":"It works"}]}]}'
```

To type several commands, open a shell in the container:

```sh
docker exec -it -u pinrail pinrail-linux bash
```

## Install more plugins

The repository's plugins are in the container at `/work/plugins`. Install
one, then send its sample:

```sh
docker exec -u pinrail pinrail-linux pinrail plugins install /work/plugins/code-review
docker exec -u pinrail pinrail-linux pinrail submit code-review --sample
```

A plugin with a build step, such as `artifact`, is built before it is
installed, which needs the network to fetch its packages:

```sh
docker exec -u pinrail -w /work/plugins/artifact pinrail-linux sh -c 'npm ci && npm run build'
docker exec -u pinrail pinrail-linux pinrail plugins install /work/plugins/artifact
```

## Try a change

The container copies your checkout when it starts. It never builds in
your working tree. To try a change, stop the container and run
`mise run linux-desktop` again. It copies the checkout again and rebuilds
only what changed.

The app's data, such as reviews and installed plugins, lives inside the
container and is gone once the container stops.

## Start from scratch

The build and the npm packages are kept in two Docker volumes. To remove
them, for example after a change to the Rust toolchain:

```sh
docker volume rm pinrail-linux-work pinrail-linux-cache
```

## Limits

The container has no GPU, and on a Mac with Apple silicon it runs Linux
for the ARM processor rather than the x86-64 of the release. Check
anything that depends on either on a real Linux machine.
