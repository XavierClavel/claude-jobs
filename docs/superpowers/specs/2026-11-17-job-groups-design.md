# Reorder jobs and put them in groups

## Why

The sidebar lists jobs in the order they were created (the order of `jobs.json`), with no way to change it. With 7
jobs, two families stand out (monitors, personal routines). The user wants to order them and put them in groups
that collapse, to find their way around.

## Goals

- Drag a job to a new place in the sidebar, into a group, out of a group, or into another group.
- Create, rename, delete and reorder groups (a group is dragged as one block).
- Collapse and expand a group. The state is kept across launches.
- A collapsed group still shows that something is wrong: its header has the job count and a dot for the worst
  state among its jobs (running, failed, waiting...), using `jobTone`.

## Non-goals and anti-goals

- Groups change nothing about how jobs run: no pausing or starting a group.
- No nested groups: one level only.
- `jobs.json` does not change. Its format is shared with `claude_jobs.py`, the `save` CLI and `status` (Dooock).
  Its order and fields stay as they are.
- The Overview keeps sorting by next run.
- Anti-goal: a job that disappears from the sidebar because the layout does not know it.

## Approaches

- **Storage. Picked: A**, a new `~/.claude-jobs/layout.json` that holds only the display. The other option, B, was a
  `group` field on each job plus the order of the `jobs.json` array (and a `groups.json` anyway for empty groups and
  their order). B was turned down because every drag would rewrite the shared `jobs.json` for a display preference.
- **Drag and drop. Picked: 1**, `@dnd-kit`. It uses pointer events, so it is not affected by Tauri's file-drop
  handling (`dragDropEnabled`, on by default on macOS, which breaks native HTML5 drag and drop). It handles nested
  sortable lists, animates them, and supports the keyboard. The other option, 2, was native HTML5 drag and drop with
  `dragDropEnabled: false`: no dependency, but a poorer result and more hand-written code for edge cases.

## Design

**`layout.json`**:

```json
{
  "ungrouped": ["job-id", "..."],
  "groups": [{ "id": "g-...", "name": "Monitors", "collapsed": false, "jobs": ["job-id", "..."] }]
}
```

- Jobs without a group are shown first, then the groups in order.
- **Reconciliation** with `jobs.json` happens each time the layout is read. An id that is no longer in `jobs.json`
  is dropped. A job that is not in the layout (new from the window or the CLI, or no layout file yet) is added at the
  bottom of the jobs without a group, in `jobs.json` order. A job listed twice is kept in its first place only. No
  file means everything is ungrouped in `jobs.json` order, which is exactly what the sidebar shows today.
- `collapsed` lives in this file, not in `localStorage`, so it lasts reliably.

**Rust**: a `layout` module with `load` / `save` / `reconcile` (and ts-rs types `Layout` and `JobGroup`).
`commands`: `get_layout` (the reconciled layout) and `save_layout(layout)` (reconciles, then writes, under the same
lock as the other writes). `api.ts` mirrors them. `delete_job` needs nothing more, since reconciliation cleans up.

**Sidebar**:
- A "New group" button (or a `+` next to the "Jobs" label) creates a "New group" that can be renamed at once.
- Double-clicking a group's name renames it inline (Enter saves, Escape cancels). A small menu on the header
  deletes the group: its jobs go back to the jobs without a group, at the bottom, in their order.
- A chevron on the header collapses or expands the group.
- Dragging a job drops it in a list (the jobs without a group, or a group). Dropping it on a collapsed group's header
  puts it at the end of that group. A group is dragged by its header.
- Drag starts after a few pixels of movement (activation constraint), so a click still selects the job.
- The sidebar is `inert` while editing (as today): no dragging then.
- The order is updated on screen right away (optimistic), then `save_layout` runs. On an error, the sidebar reloads
  the layout.

**Version**: minor bump (`feat:`) in `package.json`, `src-tauri/Cargo.toml` and `Cargo.lock`.

## Left to the builder

- How group ids are generated (as long as they are unique and stable).
- The exact look of the drop target and the drag overlay, consistent with `styles.css`.
- The breakdown of components (for example `SortableJob`, `JobGroup`).
- Tests: Rust reconciliation (unknown id, new job, duplicate, missing file), and Sidebar tests for rendering groups,
  collapsing, renaming and deleting. Drag and drop itself is tested through the pure "move" function that computes
  the new layout, rather than through jsdom.
