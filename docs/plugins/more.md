---
title: Sample plugins
description: "Sample plugins that do not come with the app: for emails, calendars, designs, logos, 3D models, audio, video, animations, palettes, before and after images, and trades."
---

Besides the plugins that come with the app, sample plugins in [forgeplane/pinrail-plugins](https://github.com/forgeplane/pinrail-plugins) show what else a plugin can do. They are not installed with Pinrail. Install the ones you need, as described below.

| Plugin | Title | What it is for |
|---|---|---|
| [`artifact`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/artifact) | HTML artifact | An HTML page an agent designed, commented on element by element, with selectors sent back to the agent. |
| [`audio`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/audio) | Audio review | Audio takes, such as voices reading one script, compared with their waveforms and transcripts, commented at a moment, and cut. |
| [`calendar`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/calendar) | Calendar | Times to arrange around what is already booked: one suggested slot picked per item, another time asked for, or the item declined. |
| [`canvas`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/canvas) | Design canvas | A set of designs on a canvas, such as the steps of a flow or variants of a screen, commented on and approved frame by frame. |
| [`email`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/email) | Draft emails | Drafts an agent wants to send, edited with the changes showing, then sent, revised or discarded one by one. |
| [`logo`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/logo) | Logo review | Candidate logo marks seen where they will live, a favourite picked, and changes asked for on parts of a mark. |
| [`model-3d`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/model-3d) | 3D model review | Candidate 3D models on a stage to orbit, a favourite picked, and changes asked for on parts of a model. |
| [`motion`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/motion) | Motion review | Candidate animations, in CSS or Lottie, played frame by frame and side by side, with a favourite picked. |
| [`palette`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/palette) | Palette review | Colour palettes or design tokens, painted on a sample screen in light and dark, with their contrast measured. |
| [`trade`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/trade) | Trade approval | A trade an agent proposes, on its candlestick chart with the entry, stop and targets to adjust, approved or rejected. |
| [`video`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/video) | Video review | A video, such as a promo cut or a screen recording, commented at a moment or over a stretch, on its picture or its sound. |
| [`visual-diff`](https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/visual-diff) | Before and after | The images of a revision, compared with a wipe, side by side, a fade or a difference highlight, each request marked fixed or not. |

Each plugin's README shows the plugin in use and describes what an agent sends and what comes back.

## Installing one

Each release of a plugin is a zip on the repository's [releases page](https://github.com/forgeplane/pinrail-plugins/releases), named after the plugin and its version, such as `email-1.0.0.zip`. Download the zip, then install it in *Settings › Plugins* with *Zip…*, or from the command line:

```sh
pinrail plugins install ~/Downloads/email-1.0.0.zip
```

You can also install a plugin from a checkout of the repository. The plugins are in its `plugins/` folder. `artifact`, `model-3d` and `motion` bundle libraries into their views, so build those first:

```sh
git clone https://github.com/forgeplane/pinrail-plugins.git
cd pinrail-plugins
pinrail plugins install ./plugins/email
(cd plugins/model-3d && npm ci && npm run build)
pinrail plugins install ./plugins/model-3d
```

[Installing plugins](/docs/using/installing-plugins/) describes what an install does, and how to upgrade a plugin or replace one.
