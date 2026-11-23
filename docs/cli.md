# CLI

Who this is for: a script author or agent calling the binary and needing exact flags, defaults and exit codes.

`scripts/docs-check.sh` checks that the help text below matches `fw-app --help` from the current build.

## Default build

```text
Reference service built from the fw-* crates

Usage: fw-app [OPTIONS] [COMMAND]

Commands:
  check  Parse and validate the config, TLS files and state directory, print a one-line summary, and exit
  serve  Run the HTTP listener. This is the default
  help   Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>  Config file (TOML) [default: fw-app.toml]
  -h, --help             Print help
  -V, --version          Print version
```

## Build with the `tui` feature

```text
Reference service built from the fw-* crates

Usage: fw-app [OPTIONS] [COMMAND]

Commands:
  check  Parse and validate the config, TLS files and state directory, print a one-line summary, and exit
  serve  Run the HTTP listener. This is the default
  top    Live terminal dashboard demo. Needs a build with the `tui` feature. Ignores --config
  help   Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>  Config file (TOML) [default: fw-app.toml]
  -h, --help             Print help
  -V, --version          Print version
```

## Behaviour

| Invocation | Effect |
|---|---|
| `fw-app` | Same as `fw-app serve` with `--config fw-app.toml`. |
| `fw-app check --config <path>` | Parse, validate, probe state, load TLS if set, print one `ok:` line, exit. |
| `fw-app serve --config <path>` | Same setup, then run the listener until SIGTERM or Ctrl-C. |
| `fw-app top` | Dashboard demo, only in `tui` builds. Ignores `--config`. Keys: `q`, Esc or Ctrl-C to quit; `p` or Space to pause. |
| `fw-app --version` | Print `fw-app 0.1.0`. |

`--config` is a global option, so it can be given before or after the subcommand. Defaults come from the code, not from the config file: an empty config file loads the same defaults listed in [configuration](configuration.md).

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Check succeeded, serve shut down cleanly, or dashboard exited normally. |
| 1 | Startup refusal, serve failure or dashboard error. The message is printed to stderr with the `fw-app: ` prefix. |

A 400, 401, 408 or 429 response is an HTTP status, not a process exit code. The listener stays up for those.

## Agent notes

Do not pass credential values on a command line. Configure the digest in the TOML file, then verify the shape with `fw-app check`. Use `--version` first to confirm which binary is on PATH, and `check` before `serve` so a config error is reported without binding a port.
