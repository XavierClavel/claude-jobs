import type { Skill } from './domain/Skill'

// The `/name` the caret is in: `start` is its slash, `end` where it stops, `query` what is typed of it so far.
export type SlashToken = { start: number; end: number; query: string }

const space = (char: string | undefined) => char !== undefined && /\s/.test(char)

// A slash that starts the prompt or follows a space, as Claude Code reads a skill. A path such as `/Users/me` is not one.
export function slashAt(text: string, caret: number): SlashToken | null {
  let start = caret
  while (start > 0 && !space(text[start - 1])) start--
  if (start === caret || text[start] !== '/') return null
  const query = text.slice(start + 1, caret)
  if (query.includes('/')) return null
  let end = caret
  while (end < text.length && !space(text[end])) end++
  return { start, end, query }
}

// First the names that start with the query, or whose part after a plugin's `name:` does; then those with a word
// that does; then those that hold it anywhere. Claude Code's order within each.
export function matching(skills: Skill[], query: string): Skill[] {
  const q = query.toLowerCase()
  const rank = (name: string) => {
    const n = name.toLowerCase()
    if (n.startsWith(q) || n.slice(n.lastIndexOf(':') + 1).startsWith(q)) return 0
    if (n.split(/[:-]/).some((word) => word.startsWith(q))) return 1
    return n.includes(q) ? 2 : -1
  }
  return skills
    .map((skill) => ({ skill, rank: rank(skill.name) }))
    .filter((m) => m.rank >= 0)
    .sort((a, b) => a.rank - b.rank)
    .map((m) => m.skill)
}

// The prompt with the token replaced by `/name `, and the caret after that space, ready for what the skill takes.
export function complete(
  text: string,
  token: SlashToken,
  name: string,
): { text: string; caret: number } {
  const before = `${text.slice(0, token.start)}/${name} `
  return { text: before + text.slice(token.end).replace(/^ /, ''), caret: before.length }
}
