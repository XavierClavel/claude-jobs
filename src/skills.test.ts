import type { Skill } from './domain/Skill'
import { complete, matching, slashAt } from './skills'

const skill = (name: string): Skill => ({ name, description: '', argument_hint: '' })

describe('slashAt', () => {
  it('finds the skill being typed at the start of the prompt or after a space', () => {
    expect(slashAt('/pho', 4)).toEqual({ start: 0, end: 4, query: 'pho' })
    expect(slashAt('Run /pho now', 8)).toEqual({ start: 4, end: 8, query: 'pho' })
    expect(slashAt('line\n/', 6)).toEqual({ start: 5, end: 6, query: '' })
  })

  it('takes the whole name when the caret is inside it', () => {
    expect(slashAt('/photobook', 3)).toEqual({ start: 0, end: 10, query: 'ph' })
  })

  it('finds nothing outside a skill name', () => {
    expect(slashAt('/pho now', 8)).toBeNull()
    expect(slashAt('/pho', 0)).toBeNull()
    expect(slashAt('a/b', 3)).toBeNull()
    expect(slashAt('/Users/me', 9)).toBeNull()
  })
})

describe('matching', () => {
  const skills = ['photobook-monitor', 'code-review', 'quality:quality-gate', 'superset:page'].map(
    skill,
  )
  const names = (query: string) => matching(skills, query).map((s) => s.name)

  it('lists every skill for a bare slash', () => {
    expect(names('')).toEqual(skills.map((s) => s.name))
  })

  it('puts names that start with the query first, a plugin skill by its own name too', () => {
    expect(names('pa')).toEqual(['superset:page'])
    expect(names('Q')).toEqual(['quality:quality-gate'])
    expect(names('re')).toEqual(['code-review'])
    expect(names('o')).toEqual(['photobook-monitor', 'code-review'])
  })

  it('then a word of the name, then anywhere in it', () => {
    expect(
      matching([skill('dataviz'), skill('quality:quality-gate')], 'gate').map((s) => s.name),
    ).toEqual(['quality:quality-gate'])
    expect(matching([skill('dataviz'), skill('my-viz')], 'viz').map((s) => s.name)).toEqual([
      'my-viz',
      'dataviz',
    ])
    expect(names('zz')).toEqual([])
  })
})

describe('complete', () => {
  it('replaces the typed name and leaves the caret after a space', () => {
    expect(complete('/pho', { start: 0, end: 4, query: 'pho' }, 'photobook-monitor')).toEqual({
      text: '/photobook-monitor ',
      caret: 19,
    })
  })

  it('keeps what follows, with a single space', () => {
    const text = 'Run /phot now'
    expect(complete(text, { start: 4, end: 9, query: 'ph' }, 'photobook-monitor')).toEqual({
      text: 'Run /photobook-monitor now',
      caret: 23,
    })
  })
})
