import { describe, expect, it } from 'vitest'
import { runTone } from './Chip'

describe('runTone', () => {
  it('shows a warning in the warning tone, apart from runs that worked or failed', () => {
    expect(runTone('warning')).toBe('warn')
    expect(runTone('ok')).toBe('ok')
    expect(runTone('failed')).toBe('bad')
  })
})
