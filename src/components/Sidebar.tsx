import { useState } from 'react'
import {
  type CheckActions,
  type Checks,
  canSignIn,
  conditionState,
  shortName,
  shownConditions,
} from '../conditions'
import type { Action } from '../domain/Action'
import type { JobView } from '../domain/JobView'
import type { Layout } from '../domain/Layout'
import { addGroup, newGroupId, reconcile } from '../layout'
import { ActionIcon } from './ActionIcon'
import { ConditionIcon } from './ConditionIcon'
import { Icon } from './Icon'
import { SideJobs } from './SideJobs'
import { UpdateCheck } from './UpdateCheck'

export function Sidebar({
  version,
  jobs,
  selected,
  conditionsOpen,
  actionsOpen,
  editing,
  checks,
  actions,
  actionList,
  onCheckUpdate,
  onNew,
  onSelect,
  onConditions,
  onActions,
  onOverview,
  layout,
  onLayout,
}: {
  version: string | null
  jobs: JobView[] | null
  selected: string | null
  conditionsOpen: boolean
  actionsOpen: boolean
  // The editor has the pane: the rest waits for Save or Cancel.
  editing: boolean
  checks: Checks
  actions: CheckActions
  // What jobs can do when a run ends.
  actionList: Action[]
  onCheckUpdate: () => Promise<boolean>
  onNew: () => void
  onSelect: (id: string) => void
  onConditions: () => void
  onActions: () => void
  onOverview: () => void
  // The order and groups of the jobs, `null` for all of them in no group.
  layout: Layout | null
  onLayout: (layout: Layout) => void
}) {
  // The group whose name is being typed: a new one, or one asked to be renamed.
  const [renaming, setRenaming] = useState<string | null>(null)
  const newGroup = () => {
    const id = newGroupId()
    const current = reconcile(
      layout,
      (jobs ?? []).map((j) => j.id),
    )
    onLayout(addGroup(current, id, 'New group'))
    setRenaming(id)
  }
  return (
    <aside className="side">
      <div className="side-head">
        <div className="side-title">
          <button type="button" className="app-name" title="Overview" onClick={onOverview}>
            Claude Jobs
          </button>
          {version && <span className="version mono">{version}</span>}
        </div>
        <UpdateCheck onCheck={onCheckUpdate} />
      </div>
      <div className="side-new">
        <button type="button" className="primary wide" disabled={editing} onClick={onNew}>
          <Icon name="plus" />
          New job
        </button>
      </div>
      <div className="side-count">
        <span className="lbl">Jobs{jobs ? ` · ${jobs.length}` : ''}</span>
        <button type="button" className="link" disabled={editing} onClick={newGroup}>
          New group
        </button>
      </div>
      <SideJobs
        jobs={jobs ?? []}
        layout={layout}
        selected={selected}
        checks={checks}
        editing={editing}
        renaming={renaming}
        onRenaming={setRenaming}
        onSelect={onSelect}
        onLayout={onLayout}
      />
      <section
        aria-label="Conditions"
        className={`side-conditions${conditionsOpen ? ' selected' : ''}`}
        inert={editing}
      >
        <div className="side-conditions-head">
          <span className="lbl">Conditions</span>
          {conditionsOpen ? (
            <span className="link">Managing</span>
          ) : (
            <button type="button" className="link" onClick={onConditions}>
              Manage
            </button>
          )}
        </div>
        {shownConditions(checks, jobs ?? []).map((condition) => {
          const state = conditionState(condition, checks)
          const signIn = state.met === false && canSignIn(condition, checks)
          return (
            <div key={condition} className="side-condition">
              <span
                className={`dot dot-${state.met === null ? 'muted' : state.met ? 'ok' : 'warn'}`}
              />
              <ConditionIcon condition={condition} size={13} />
              <span className="side-condition-name">{shortName(condition, checks)}</span>
              {signIn && !conditionsOpen ? (
                <button
                  type="button"
                  className="small"
                  disabled={condition === 'gcloud' && checks.signingIn}
                  onClick={() => actions.onSignIn(condition)}
                >
                  {condition === 'gcloud' && checks.signingIn ? 'Signing in…' : 'Sign in'}
                </button>
              ) : (
                <span className="muted small-text">{state.word}</span>
              )}
            </div>
          )
        })}
      </section>
      <section
        aria-label="Actions"
        className={`side-conditions${actionsOpen ? ' selected' : ''}`}
        inert={editing}
      >
        <div className="side-conditions-head">
          <span className="lbl">Actions</span>
          {actionsOpen ? (
            <span className="link">Managing</span>
          ) : (
            <button type="button" className="link" onClick={onActions}>
              Manage
            </button>
          )}
        </div>
        {actionList.length === 0 ? (
          <span className="muted small-text">None yet</span>
        ) : (
          actionList.map((action) => (
            <div key={action.id} className="side-condition">
              <ActionIcon action={action} size={13} />
              <span className="side-condition-name">{action.name}</span>
              <span className="muted small-text">Slack</span>
            </div>
          ))
        )}
      </section>
    </aside>
  )
}
