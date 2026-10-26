# Claude Jobs

A macOS app that runs Claude Code prompts on a schedule, headless, and shows how each run went.

Each job is a prompt (such as `/photobook-monitor`), a time and days, and the tools it may use without asking. The app
turns it into a launchd agent that calls the app's own binary at that time:

```
~/Library/LaunchAgents/com.claude-jobs.<job>.plist  →  Claude Jobs.app/Contents/MacOS/claude-jobs run <job>
                                                    →  claude -p <prompt> --output-format json --allowedTools …
```

Runs happen whether the app is open or not. launchd runs a missed time when the Mac wakes; it does not run while the
Mac is off or you are logged out.

## Files

Everything lives in `~/.claude-jobs`, the same folder as the earlier `claude_jobs.py` page:

- `jobs.json`: the jobs.
- `runs/<job>/<run>.json`: one record per run (status, duration, turns, cost, Claude's result, session id), with
  `<run>.out` (Claude's JSON output) and `<run>.err` (its stderr) beside it. The last 100 runs are kept.
- `logs/<job>.launchd.log`: what the runner itself printed.

On start, the app points every enabled job's launchd file at its own binary, so moving or updating the app keeps the
schedules working.

## Stack

Tauri 2 (Rust) with React 19, TypeScript and Vite; pnpm, Biome, Vitest and Testing Library. The TypeScript types in
`src/domain` are generated from the Rust structs with `ts-rs`: run `pnpm domain:build` after changing one.

## Develop

```sh
pnpm install
pnpm tauri dev          # the app, reloading on change (a dev build never touches the launchd files)
pnpm test               # front-end tests
pnpm test:rust          # Rust tests
pnpm lint && pnpm typecheck
pnpm app:build          # src-tauri/target/release/bundle/macos/Claude Jobs.app
```

## Interactive jobs

A headless `claude -p` session has no Artifact tools. A job marked **Interactive session** runs a real Claude Code
session instead, in its own tmux server (`tmux -L claude-jobs`, so it never mixes with yours). A Stop hook passed
with `--settings` tells the runner when Claude has finished; the runner then sends `/exit` and reads the result,
turns and cost from the session transcript. While it runs, **Watch** opens Terminal on the session. Needs
`brew install tmux`.

## Limits

- An interactive job waits on any permission prompt until its timeout: allow every tool it needs (your default
  permission mode still applies to the rest).
- A job runs with the PATH set in its advanced settings, not your shell's: add the folders of every command it needs.
