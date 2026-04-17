# @erchoc/cb

Cross-platform voice assistant that lives in your terminal.

```bash
npm install -g @erchoc/cb
cb --version
```

Equivalent to `brew install erchoc/tap/cb`. Both channels ship the same
binary from the [Erchoc/chatbot](https://github.com/Erchoc/chatbot) GitHub
Release.

## Supported platforms

- macOS (arm64 + x64, universal binary)
- Linux (x64, arm64)

Windows is not supported yet — the command prints a friendly error.

## How this package works

The published tarball contains the pre-built native binaries for every
supported platform alongside a small Node.js launcher. The launcher picks
the right binary at runtime based on `process.platform` / `process.arch`
and execs it with stdio inheritance. There is no `postinstall` step and no
network access at install time.

For the full packaging spec see
[homebrew-tap/docs/npm-convention.md](https://github.com/Erchoc/homebrew-tap/blob/master/docs/npm-convention.md).

## License

MIT. See `LICENSE`.

## Issues

Please report issues at <https://github.com/Erchoc/chatbot/issues>.
