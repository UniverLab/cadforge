# cadspec demo

The cadspec demo as code, recorded with [DemoStage](https://github.com/UniverLab/demo-stage).
It shows the terminal scaffolding a CAD project and starting the live preview, with
the rendered drawing in a browser pane.

## Regenerate

```sh
demo check  demo/demo.toml
demo export demo/demo.toml --target gif   # → demo/dist/cadspec.gif
# or --target mp4
```

- `gif`/`mp4` of a terminal + browser pane need a Chromium (auto-downloaded by
  DemoStage on first use, or use a system install) and, for mp4, ffmpeg.
- The browser pane is captured after the terminal run, so this score leaves the
  `cadspec serve` server running; stop it afterwards with
  `cd /tmp/cadspec-demo/bracket && cadspec serve --stop`.

Edit `demo/demo.toml` to change the commands, captions, layout or timing.
