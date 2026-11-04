import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { api } from '../api'
import { action, command, gcloudOut, recipient, signedIn, use } from '../test/fixtures'
import { JobEditor } from './JobEditor'

vi.mock('../api', () => ({
  api: {
    saveJob: vi.fn(),
    saveAction: vi.fn(),
    listSkills: vi.fn(() => Promise.resolve([])),
    listRepos: vi.fn(() =>
      Promise.resolve([
        {
          name: 'Pictarine/web-apps',
          private: true,
          pushed_at: '2026-11-04T08:58:21Z',
          owner_avatar: 'https://avatars.githubusercontent.com/u/1?v=4',
        },
        {
          name: 'XavierClavel/claude-jobs',
          private: false,
          pushed_at: '2026-11-03T11:37:24Z',
          owner_avatar: 'https://avatars.githubusercontent.com/u/2?v=4',
        },
        {
          name: 'Pictarine/backend-picta-api',
          private: true,
          pushed_at: null,
          owner_avatar: 'https://avatars.githubusercontent.com/u/1?v=4',
        },
      ]),
    ),
  },
  errorsOf: () => [],
}))

const defaults = { claude: '/bin/claude', path: '/usr/bin:/bin', cwd: '/Users/me' }

async function newJob() {
  const onSaved = vi.fn()
  render(
    <JobEditor
      job={null}
      defaults={defaults}
      checks={signedIn}
      actions={[]}
      onActionSaved={() => {}}
      onClose={() => {}}
      onSaved={onSaved}
      onDeleted={() => {}}
    />,
  )
  await userEvent.type(screen.getByLabelText('Name'), 'Monthly report')
  await userEvent.type(screen.getByLabelText('Prompt'), '/report')
  return onSaved
}

describe('JobEditor', () => {
  beforeEach(() => vi.mocked(api.saveJob).mockReset())

  it('starts daily, with no days to pick', async () => {
    await newJob()
    expect(screen.getByRole('radio', { name: 'Daily' })).toBeChecked()
    expect(screen.queryByRole('group', { name: 'Days of the week' })).not.toBeInTheDocument()
  })

  it('saves a job a merged pull request starts, its repositories picked from a search', async () => {
    await newJob()
    await userEvent.click(screen.getByRole('radio', { name: 'On trigger' }))
    expect(screen.getByRole('radio', { name: 'Pull request opened' })).toBeChecked()
    await userEvent.click(screen.getByRole('radio', { name: 'Pull request merged' }))
    expect(screen.queryByRole('radio', { name: 'Daily' })).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Add repository' }))
    const search = screen.getByRole('combobox', { name: 'Search repositories' })
    const options = await screen.findAllByRole('option')
    expect(options).toHaveLength(3)
    // Each with its owner's picture, small.
    expect(options[1]?.querySelector('img')).toHaveAttribute(
      'src',
      'https://avatars.githubusercontent.com/u/2?v=4&s=40',
    )
    await userEvent.type(search, 'picta api')
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual([
      'Pictarine/backend-picta-apiPrivate',
    ])
    await userEvent.type(search, '{enter}')
    await userEvent.click(screen.getByRole('button', { name: 'Add repository' }))
    // One already added is not offered again.
    expect(screen.getAllByRole('option')).toHaveLength(2)
    await userEvent.click(screen.getByRole('option', { name: /claude-jobs/ }))
    expect(screen.getByLabelText('Only my pull requests')).toBeChecked()
    await userEvent.click(
      screen.getByRole('button', { name: 'Remove Pictarine/backend-picta-api' }),
    )
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveJob).toHaveBeenCalledWith(
      expect.objectContaining({
        event: { kind: 'pull_request_merged', repos: ['XavierClavel/claude-jobs'], mine: true },
      }),
      null,
    )
  })

  it('picks days of the week for a weekly job, starting on Monday', async () => {
    await newJob()
    await userEvent.click(screen.getByRole('radio', { name: 'Weekly' }))
    expect(screen.getByLabelText('Mon')).toBeChecked()
    await userEvent.click(screen.getByLabelText('Fri'))
    expect(screen.getByLabelText('Fri')).toBeChecked()
  })

  it('saves a job that runs on days of the month', async () => {
    const onSaved = await newJob()
    await userEvent.click(screen.getByRole('radio', { name: 'Monthly' }))
    expect(screen.queryByRole('group', { name: 'Days of the week' })).not.toBeInTheDocument()
    await userEvent.click(screen.getByLabelText('15'))
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveJob).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'monthly-report', weekdays: [], month_days: [1, 15] }),
      null,
    )
    expect(onSaved).toHaveBeenCalledOnce()
  })

  it('can run on the last day of shorter months instead', async () => {
    await newJob()
    await userEvent.click(screen.getByRole('radio', { name: 'Monthly' }))
    expect(screen.queryByLabelText('Run on the last day of shorter months')).not.toBeInTheDocument()
    await userEvent.click(screen.getByLabelText('31'))
    await userEvent.click(screen.getByLabelText('Run on the last day of shorter months'))
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveJob).toHaveBeenCalledWith(
      expect.objectContaining({ month_days: [1, 31], last_day_fallback: true }),
      null,
    )
  })

  it('has no catch-up option: every job catches up', async () => {
    await newJob()
    expect(screen.queryByLabelText(/Catch up/)).not.toBeInTheDocument()
  })

  it('wants a day before saving', async () => {
    await newJob()
    await userEvent.click(screen.getByRole('radio', { name: 'Weekly' }))
    await userEvent.click(screen.getByLabelText('Mon'))
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(screen.getByRole('alert')).toHaveTextContent('Pick at least one day of the week.')
    expect(api.saveJob).not.toHaveBeenCalled()
  })
})

describe('JobEditor conditions', () => {
  beforeEach(() => vi.mocked(api.saveJob).mockReset())

  it('adds Google Cloud from the picker, says it is not met, and removes it', async () => {
    render(
      <JobEditor
        job={null}
        defaults={defaults}
        checks={gcloudOut}
        actions={[]}
        onActionSaved={() => {}}
        onClose={() => {}}
        onSaved={() => {}}
        onDeleted={() => {}}
      />,
    )
    expect(screen.getByText(/Required by every job · Met/)).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Add condition' }))
    await userEvent.click(screen.getByRole('menuitemcheckbox', { name: /Google Cloud/ }))
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
    expect(screen.getByText(/Not met right now/)).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Remove Google Cloud signed in' }))
    expect(screen.queryByText(/Not met right now/)).not.toBeInTheDocument()
  })

  it('offers the command conditions, with those for every job always on', async () => {
    render(
      <JobEditor
        job={null}
        defaults={defaults}
        checks={{
          ...signedIn,
          commands: [
            command({ id: 'vpn', name: 'VPN connected', every_job: true }),
            command({ id: 'db', name: 'Database up', problem: 'Start the database.' }),
          ],
        }}
        actions={[]}
        onActionSaved={() => {}}
        onClose={() => {}}
        onSaved={() => {}}
        onDeleted={() => {}}
      />,
    )
    expect(screen.getAllByText(/Required by every job/)).toHaveLength(2)
    expect(screen.getByText('VPN connected')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Add condition' }))
    expect(screen.getByRole('menuitemcheckbox', { name: /VPN connected/ })).toHaveAttribute(
      'aria-disabled',
      'true',
    )
    await userEvent.click(screen.getByRole('menuitemcheckbox', { name: /Database up/ }))
    expect(screen.getByRole('button', { name: 'Remove Database up' })).toBeInTheDocument()
    expect(screen.getByText(/Not met right now/)).toBeInTheDocument()
  })

  it('saves the conditions picked', async () => {
    const onSaved = await newJob()
    await userEvent.click(screen.getByRole('button', { name: 'Add condition' }))
    await userEvent.click(screen.getByRole('menuitemcheckbox', { name: /Google Cloud/ }))
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveJob).toHaveBeenCalledWith(
      expect.objectContaining({ conditions: ['gcloud'] }),
      null,
    )
    expect(onSaved).toHaveBeenCalledOnce()
  })
})

describe('JobEditor actions', () => {
  beforeEach(() => vi.mocked(api.saveJob).mockReset())

  const me = recipient({ id: 'me', name: 'Xavier', slack_id: 'U0123ABCD' })
  const alert = action({ recipients: [recipient(), me] })
  const digest = action({ id: 'digest', name: 'Digest workflow', recipients: [me] })

  function editor(actions = [alert, digest]) {
    const onActionSaved = vi.fn()
    render(
      <JobEditor
        job={null}
        defaults={defaults}
        checks={signedIn}
        actions={actions}
        onActionSaved={onActionSaved}
        onClose={() => {}}
        onSaved={() => {}}
        onDeleted={() => {}}
      />,
    )
    return onActionSaved
  }

  async function named() {
    await userEvent.type(screen.getByLabelText('Name'), 'Monthly report')
    await userEvent.type(screen.getByLabelText('Prompt'), '/report')
  }

  it('picks an action and which of its recipients it sends to, and saves them', async () => {
    editor()
    await named()
    await userEvent.click(screen.getByRole('button', { name: 'Add action on failure' }))
    const menu = within(screen.getByRole('menu', { name: 'Actions on failure' }))
    // Each action by its name alone.
    expect(menu.getAllByRole('menuitem').map((m) => m.textContent)).toEqual([
      'Alert workflow',
      'Digest workflow',
    ])
    await userEvent.click(menu.getByRole('menuitem', { name: 'Alert workflow' }))
    const toWhom = screen.getByRole('combobox', { name: 'Who Alert workflow sends to on failure' })
    expect(toWhom).toHaveValue('')
    // The action's own recipients, and a way to add one.
    expect(
      within(toWhom)
        .getAllByRole('option')
        .map((o) => o.textContent),
    ).toEqual(['No recipient', 'Ops channel', 'Xavier', 'New recipient…'])
    await userEvent.selectOptions(toWhom, 'ops')
    // The same action again, to someone else: only Xavier is left, so it is picked.
    await userEvent.click(screen.getByRole('button', { name: 'Add action on failure' }))
    await userEvent.click(screen.getByRole('menuitem', { name: /Alert workflow/ }))
    const failure = within(screen.getByRole('group', { name: 'On failure' }))
    expect(failure.getAllByRole('combobox').map((c) => (c as HTMLSelectElement).value)).toEqual([
      'ops',
      'me',
    ])
    // An action with one recipient sends to it at once.
    await userEvent.click(screen.getByRole('button', { name: 'Add action on success' }))
    await userEvent.click(screen.getByRole('menuitem', { name: /Digest workflow/ }))
    const [first] = failure.getAllByRole('button', { name: 'Remove Alert workflow on failure' })
    if (first) await userEvent.click(first)
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveJob).toHaveBeenCalledWith(
      expect.objectContaining({
        on_success: [use('me', 'digest')],
        on_failure: [use('me')],
        on_complete: [],
      }),
      null,
    )
  })

  it('uses an action with no recipient', async () => {
    editor([action({ recipients: [] })])
    await named()
    await userEvent.click(screen.getByRole('button', { name: 'Add action on success' }))
    await userEvent.click(screen.getByRole('menuitem', { name: 'Alert workflow' }))
    expect(
      screen.getByRole('combobox', { name: 'Who Alert workflow sends to on success' }),
    ).toHaveValue('')
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveJob).toHaveBeenCalledWith(
      expect.objectContaining({ on_success: [use('')] }),
      null,
    )
  })

  it('lets Claude send to anyone during the run, and only then', async () => {
    editor()
    await named()
    await userEvent.click(screen.getByRole('button', { name: 'Add action during the run' }))
    await userEvent.click(screen.getByRole('menuitem', { name: 'Alert workflow' }))
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Who Alert workflow sends to during the run' }),
      'Anyone',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Add action on success' }))
    await userEvent.click(screen.getByRole('menuitem', { name: 'Alert workflow' }))
    const onSuccess = screen.getByRole('combobox', {
      name: 'Who Alert workflow sends to on success',
    })
    expect(within(onSuccess).queryByRole('option', { name: 'Anyone' })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveJob).toHaveBeenCalledWith(
      expect.objectContaining({ during_run: [use('*')] }),
      null,
    )
  })

  it('picks who Claude can send to during the run, and saves it', async () => {
    editor()
    await named()
    await userEvent.click(screen.getByRole('button', { name: 'Add action during the run' }))
    await userEvent.click(screen.getByRole('menuitem', { name: /Alert workflow/ }))
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Who Alert workflow sends to during the run' }),
      'ops',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveJob).toHaveBeenCalledWith(
      expect.objectContaining({ during_run: [use()], on_success: [], on_failure: [] }),
      null,
    )
  })

  it('adds a recipient to the action from the menu, and picks it', async () => {
    vi.mocked(api.saveAction).mockImplementation((a) => Promise.resolve(a))
    const onActionSaved = editor([action()])
    await named()
    await userEvent.click(screen.getByRole('button', { name: 'Add action on success' }))
    await userEvent.click(screen.getByRole('menuitem', { name: /Alert workflow/ }))
    const toWhom = screen.getByRole('combobox', { name: 'Who Alert workflow sends to on success' })
    expect(toWhom).toHaveValue('ops')
    await userEvent.selectOptions(toWhom, 'New recipient…')
    const form = within(screen.getByRole('group', { name: 'New recipient of Alert workflow' }))
    await userEvent.type(form.getByLabelText('Name'), 'Ops')
    await userEvent.type(form.getByLabelText('Slack ID'), 'C0999ZZZZ')
    await userEvent.click(form.getByRole('button', { name: 'Add recipient' }))
    // Saved in the action, with an id the action has not given yet.
    const saved = action({
      recipients: [recipient(), { id: 'ops-2', name: 'Ops', slack_id: 'C0999ZZZZ' }],
    })
    expect(api.saveAction).toHaveBeenCalledWith(saved, 'alert')
    expect(onActionSaved).toHaveBeenCalledWith(saved)
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.saveJob).toHaveBeenCalledWith(
      expect.objectContaining({ on_success: [use('ops-2')] }),
      null,
    )
  })

  it('has nothing to pick before an action exists', () => {
    editor([])
    expect(screen.getByRole('button', { name: 'Add action during the run' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Add action on success' })).toBeDisabled()
    expect(screen.getAllByText(/No action yet/)).toHaveLength(2)
  })
})

describe('JobEditor prompt', () => {
  const skills = [
    { name: 'photobook-monitor', description: 'Refresh the photobook monitor', argument_hint: '' },
    { name: 'prompt-review:prompt-review', description: 'Grade recent prompts', argument_hint: '' },
    { name: 'code-review', description: 'Review the diff', argument_hint: '[low|high]' },
  ]

  async function openEditor() {
    vi.mocked(api.listSkills).mockClear().mockResolvedValue(skills)
    render(
      <JobEditor
        job={null}
        defaults={defaults}
        checks={signedIn}
        actions={[]}
        onActionSaved={() => {}}
        onClose={() => {}}
        onSaved={() => {}}
        onDeleted={() => {}}
      />,
    )
    await waitFor(() =>
      expect(api.listSkills).toHaveBeenCalledWith('/bin/claude', '/usr/bin:/bin', '/Users/me'),
    )
    return screen.getByLabelText('Prompt')
  }

  it('suggests the skills that match what follows the slash', async () => {
    const prompt = await openEditor()
    await userEvent.type(prompt, '/')
    expect(screen.getAllByRole('option')).toHaveLength(3)
    await userEvent.type(prompt, 'pr')
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual([
      '/prompt-review:prompt-reviewGrade recent prompts',
    ])
  })

  it('puts in the skill picked with the arrows and Enter', async () => {
    const prompt = await openEditor()
    await userEvent.type(prompt, '/re')
    expect(screen.getByRole('option', { selected: true })).toHaveTextContent('/prompt-review')
    await userEvent.keyboard('{ArrowDown}')
    expect(screen.getByRole('option', { selected: true })).toHaveTextContent('/code-review')
    await userEvent.keyboard('{Enter}')
    expect(prompt).toHaveValue('/code-review ')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
    await userEvent.type(prompt, 'high')
    expect(prompt).toHaveValue('/code-review high')
  })

  it('puts in the skill clicked', async () => {
    const prompt = await openEditor()
    await userEvent.type(prompt, 'Run /pho')
    await userEvent.click(screen.getByRole('option', { name: /photobook-monitor/ }))
    expect(prompt).toHaveValue('Run /photobook-monitor ')
    expect(prompt).toHaveFocus()
  })

  it('hides the list on Escape, and shows it again on the next key', async () => {
    const prompt = await openEditor()
    await userEvent.type(prompt, '/p')
    await userEvent.keyboard('{Escape}')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
    await userEvent.type(prompt, 'h')
    expect(screen.getByRole('option')).toHaveTextContent('/photobook-monitor')
  })
})
