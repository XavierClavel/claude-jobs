import {
  conditionState,
  fullName,
  optionalConditions,
  required,
  shownConditions,
  stateTitle,
  usedBy,
  waitsFor,
} from './conditions'
import { command, job, signedIn } from './test/fixtures'

const vpn = command({ id: 'vpn', every_job: true })
const db = command({
  id: 'db',
  name: 'Database up',
  expect: { kind: 'exit_code', code: 0 },
  problem: 'Start the database: the command exited with 2, not 0.',
})
const checks = { ...signedIn, commands: [vpn, db] }

describe('command conditions', () => {
  it('are required by every job, or by the jobs that tick them, after Claude Code', () => {
    expect(required(job(), checks)).toEqual(['claude', 'vpn'])
    expect(required(job({ conditions: ['db', 'gcloud', 'vpn'] }), checks)).toEqual([
      'claude',
      'vpn',
      'db',
      'gcloud',
    ])
    // One deleted since the job ticked it holds nothing back.
    expect(required(job({ conditions: ['gone'] }), checks)).toEqual(['claude', 'vpn'])
  })

  it('are used by every job, or by the jobs that tick them', () => {
    const jobs = [job({ id: 'a', conditions: ['db'] }), job({ id: 'b' })]
    expect(usedBy('vpn', jobs, checks).map((j) => j.id)).toEqual(['a', 'b'])
    expect(usedBy('db', jobs, checks).map((j) => j.id)).toEqual(['a'])
  })

  it('can be ticked unless they are on every job already', () => {
    expect(optionalConditions(checks)).toEqual(['gcloud', 'db'])
  })

  it('are shown after the built-in ones, with their state and names', () => {
    expect(shownConditions(checks, [])).toEqual(['claude', 'gcloud', 'vpn', 'db'])
    expect(conditionState('vpn', checks)).toEqual({ met: true, word: 'met', problem: null })
    expect(conditionState('db', checks).met).toBe(false)
    expect(stateTitle('db', checks)).toBe('Database up is not met')
    expect(fullName('gcloud', checks)).toBe('Google Cloud signed in')
    expect(fullName('db', checks)).toBe('Database up')
    expect(
      waitsFor(
        [
          { condition: 'gcloud', problem: 'x' },
          { condition: 'db', problem: 'y' },
        ],
        checks,
      ),
    ).toBe('Google Cloud and Database up')
  })
})
