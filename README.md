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

A job with **Catch up a missed run** makes up for that: while one is enabled, a login agent
(`com.claude-jobs.app.catch-up`, calling `claude-jobs catch-up`) checks each such job at login and starts it when its
last scheduled time has passed with no run since, as long as that time is less than a day old. It waits up to 5 minutes
for the network first. Loading the agent also runs it, so ticking the box after a missed time catches it up at once.
Catch-up runs show as **Catch-up** in the history; what the agent did is in `logs/app.catch-up.launchd.log`.

## Requirements

- Claude Code, signed in (run `claude` once).
- tmux for interactive jobs. When it is missing, ticking **Interactive session** in the editor offers to install it
  with Homebrew (`brew install tmux`, in the background; what Homebrew printed is in `logs/tmux-install.log`).
- The gcloud CLI for jobs that read Google Cloud.

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
turns and cost from the session transcript. While it runs, **Watch** opens Terminal on the session, or brings forward the Terminal tab already showing it (macOS
asks once to let Claude Jobs control Terminal; without that, it opens a new window). Needs
tmux: the editor installs it with Homebrew when it is missing, and refuses an interactive job without it.

In a folder it does not trust yet, Claude Code opens on "Is this a project you trust?" and waits for an answer. Saving
an interactive job, and starting its runs, records the working folder as trusted the way Claude Code describes:
`projects[<folder>].hasTrustDialogAccepted` in `~/.claude.json`, written under Claude Code's own lock
(`~/.claude.json.lock`). Claude Code never keeps that answer for the home folder, and the app does not write it either
(it would trust every folder in it): an interactive job needs a folder of its own. Should the question still show, the
run stops at once instead of waiting for its timeout.

## Google Cloud

Jobs that read BigQuery or Cloud Logging fail once gcloud's credentials expire ("Reauthentication failed"). The
window checks them at start, every 5 minutes and when it comes back to the front, by printing an access token, and
shows a warning when they are gone. **Sign in to Google Cloud** runs `gcloud auth login --update-adc` in the
background: the browser opens, and the bar turns green once you have signed in there (it gives up after 5 minutes).
That refreshes the application default credentials too.

## Limits

- An interactive job waits on any permission prompt until its timeout: allow every tool it needs (your default
  permission mode still applies to the rest).
- A job runs with the PATH set in its advanced settings, not your shell's: add the folders of every command it needs.
