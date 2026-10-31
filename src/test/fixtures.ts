import type { Checks } from '../conditions'
import type { CommandConditionView } from '../domain/CommandConditionView'
import type { JobView } from '../domain/JobView'
import type { RunRecord } from '../domain/RunRecord'

export function job(overrides: Partial<JobView> = {}): JobView {
  return {
    id: 'photobook-monitor',
    name: 'Photobook monitor',
    prompt: '/photobook-monitor',
    hour: 9,
    minute: 0,
    weekdays: [],
    month_days: [],
    last_day_fallback: false,
    cwd: '/Users/me',
    allowed_tools: [],
    model: '',
    timeout_minutes: 30,
    enabled: true,
    claude: '/bin/claude',
    path: '/usr/bin',
    interactive: false,
    chrome: false,
    conditions: [],
    label: 'com.claude-jobs.photobook-monitor',
    loaded: true,
    next_run: '2026-10-27T09:00:00+01:00',
    last_run: null,
    waiting: null,
    unmet_conditions: [],
    ...overrides,
  }
}

export function run(overrides: Partial<RunRecord> = {}): RunRecord {
  return {
    id: '20261026T134651Z',
    job: 'photobook-monitor',
    trigger: 'schedule',
    status: 'ok',
    started_at: '2026-10-26T13:46:51Z',
    ended_at: '2026-10-26T13:48:32Z',
    duration_s: 101,
    exit_code: 0,
    cost_usd: 0.65,
    num_turns: 23,
    session_id: null,
    result: 'done',
    error: null,
    pid: 1,
    claude_pid: null,
    prompt: '/photobook-monitor',
    interactive: false,
    tmux_session: null,
    ...overrides,
  }
}

export const signedIn: Checks = {
  claude: { condition: 'claude', problem: null },
  gcloud: { installed: true, signed_in: true, account: 'me@example.com' },
  signingIn: false,
  commands: [],
}

export const gcloudOut: Checks = {
  ...signedIn,
  gcloud: { installed: true, signed_in: false, account: null },
}

export const expired = {
  condition: 'claude' as const,
  problem: "Claude Code's login expired: run claude, then /login.",
}

export function command(overrides: Partial<CommandConditionView> = {}): CommandConditionView {
  return {
    id: 'vpn',
    name: 'VPN connected',
    command: 'scutil --nc status Work | head -1',
    expect: { kind: 'stdout', text: 'Connected', contains: false },
    every_job: false,
    hint: 'Connect to the VPN',
    problem: null,
    checked_at: '2026-10-31T09:00:00Z',
    ...overrides,
  }
}
