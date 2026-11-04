import type { MouseEvent, ReactNode } from 'react'
import { api } from '../api'
import type { PullRequest } from '../domain/PullRequest'
import { OwnerAvatar } from './RepoPicker'

// A repository a job watches, with its owner's picture: opens it on GitHub.
export function RepoChip({ repo }: { repo: string }) {
  return (
    <GitHubChip repo={repo} title={`Open ${repo} on GitHub`} onOpen={() => api.openOnGithub(repo)}>
      {shortName(repo)}
    </GitHubChip>
  )
}

// The pull request that started a run: opens it on GitHub.
export function PullRequestChip({ pr }: { pr: PullRequest }) {
  return (
    <GitHubChip
      repo={pr.repo}
      title={`${pr.title}, by ${pr.author}: open on GitHub`}
      onOpen={() => api.openOnGithub(pr.repo, pr.number)}
    >
      #{pr.number} {shortName(pr.repo)}
    </GitHubChip>
  )
}

function GitHubChip({
  repo,
  title,
  onOpen,
  children,
}: {
  repo: string
  title: string
  onOpen: () => Promise<void>
  children: ReactNode
}) {
  return (
    <button
      type="button"
      className="repo-chip"
      title={title}
      onClick={(e: MouseEvent) => {
        // Not the row it may sit in.
        e.stopPropagation()
        onOpen().catch(() => {})
      }}
      onKeyDown={(e) => e.stopPropagation()}
    >
      <OwnerAvatar repo={undefined} name={repo} />
      {children}
    </button>
  )
}

const shortName = (repo: string) => repo.split('/').pop() ?? repo
