# Reorder jobs and put them in groups: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorder the jobs in the sidebar by drag and drop, and put them in named groups that collapse.

**Architecture:** A new `~/.claude-jobs/layout.json`, owned by a Rust `layout` module, holds the order and the
groups. It is reconciled with `jobs.json` on every read and every write. The front end edits it through pure
functions in `src/layout.ts`, shows it in `Sidebar`, and saves it optimistically. Drag and drop uses `@dnd-kit`.

**Tech Stack:** Rust (serde, ts-rs, Tauri 2 commands), React 19, TypeScript, `@dnd-kit/core` +
`@dnd-kit/sortable` + `@dnd-kit/utilities`, vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-11-17-job-groups-design.md`

## Global Constraints

- `jobs.json` stays unchanged: same format, same order, no new field. `cli status` stays unchanged.
- `src/domain` is generated (`pnpm domain:build`) and never edited by hand.
- A dev build must not rewrite the launchd files. This feature does not touch them.
- Version 0.12.0 → **0.13.0** in `package.json`, `src-tauri/Cargo.toml` and the `claude-jobs` entry of
  `src-tauri/Cargo.lock`. If main moved past 0.12.0, bump the minor past it.
- Before each commit: `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm test:rust`, then `pnpm domain:build` (or
  `pnpm lint:fix`) so `src/domain` is formatted.
- If a test fails, stop and ask the user before trying again (user rule).
- Copy (English, like the rest of the UI): "New group", "Rename", "Delete group", "No jobs" (empty group).

## Review Focus

1. **The jobs list refreshes every 4 s during a run**: a drag in progress must not be cancelled or jump around. The
   layout is separate state that the polling never replaces. → test in Task 3 (`keeps the layout when the jobs refresh`).
2. **A job created from the CLI, or deleted**: it must show up (at the bottom of the jobs without a group) or
   disappear on the next refresh, without restarting the app. The sidebar shows the layout reconciled on the
   front end too, so it never depends on a `get_layout` being up to date. → `reconcile` tests in Task 2.
3. **A `layout.json` that does not parse**: no job disappears. The banner shows the error and the sidebar shows every
   job ungrouped. The next save rewrites the file (it is display only). → Rust test in Task 1, Sidebar test in Task 3.
4. **Renaming to an empty or blank name** keeps the old name. Escape cancels. → Task 3.
5. **Dropping a job on an empty group, or on a collapsed group**: the job goes into it (at the end). → `moveJob`
   tests in Task 2 and a droppable on the group's body or header in Task 4.

---

### Task 1: `layout` module and commands (Rust)

**Files:**
- Create: `src-tauri/src/layout.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod layout;` and the two commands in `generate_handler!`),
  `src-tauri/src/paths.rs` (`layout_file()`), `src-tauri/src/commands.rs`, `src/api.ts`
- Modify: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (0.13.0), `CLAUDE.md` (add `layout` to the
  list of modules)
- Generated: `src/domain/Layout.ts`, `src/domain/JobGroup.ts`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)] #[ts(export)]
  pub struct Layout { #[serde(default)] pub ungrouped: Vec<String>, #[serde(default)] pub groups: Vec<JobGroup> }
  #[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)] #[ts(export)]
  pub struct JobGroup { pub id: String, pub name: String, #[serde(default)] pub collapsed: bool,
                        #[serde(default)] pub jobs: Vec<String> }
  pub fn load() -> Result<Layout, String>      // no file → Layout::default(); unparsable → Err("layout.json does not parse: …")
  pub fn save(layout: &Layout) -> Result<(), String>   // tmp file + rename, like jobs::save
  pub fn reconcile(layout: Layout, job_ids: &[String]) -> Layout
  // commands.rs
  pub fn get_layout() -> Reply<Layout>                  // reconcile(load()?, ids of jobs::load()?)
  pub fn save_layout(layout: Layout) -> Reply<Layout>   // under lock(): reconcile, save, return what was saved
  ```
  `api.ts`: `getLayout: () => invoke<Layout>('get_layout')`,
  `saveLayout: (layout: Layout) => invoke<Layout>('save_layout', { layout })`.

`reconcile` rules: drop ids that are not in `job_ids`. Keep only the first occurrence of an id, scanning `ungrouped`
then the groups in order. Drop a group whose `id` repeats an earlier group's id (its jobs fall through to the
missing-job rule). Append the jobs that are not placed to `ungrouped`, in `job_ids` order. Keep empty groups.

- [ ] **Step 1: Write the failing tests** in `layout.rs` `#[cfg(test)]`:
  - `reconcile_with_no_layout_lists_every_job_ungrouped_in_order`: `reconcile(Layout::default(), &["a","b"])` → `ungrouped == ["a","b"]`, no groups.
  - `reconcile_drops_jobs_that_no_longer_exist`: group `g` with `["a","x"]`, ids `["a"]` → `g.jobs == ["a"]`.
  - `reconcile_appends_new_jobs_ungrouped`: ungrouped `["b"]`, group with `["a"]`, ids `["a","b","c"]` → ungrouped `["b","c"]`.
  - `reconcile_keeps_a_job_listed_twice_in_its_first_place`: ungrouped `["a"]`, group `["a"]` → group jobs `[]`.
  - `reconcile_keeps_empty_groups_and_their_state`: group `collapsed: true`, `jobs: []` is kept as it is.
  - `reconcile_drops_a_repeated_group_id`.
  - `layout_parses_without_optional_fields`: `{"groups":[{"id":"g","name":"G"}]}` parses, `collapsed == false`, `ungrouped` empty.
  - `load_refuses_a_file_that_does_not_parse`: if the module tests can point `HOME` at a temp dir the way other tests
    do, check that `load()` is `Err` containing "does not parse". Otherwise test a `parse(text: &str) -> Result<Layout, String>`
    helper that `load` uses.
- [ ] **Step 2: Run** `pnpm test:rust layout`. Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `layout.rs`, `paths::layout_file()` (`base().join("layout.json")`), the two commands,
  their registration, and `api.ts`.
- [ ] **Step 4: Run** `pnpm test:rust`, then `pnpm domain:build`, `pnpm lint`, `pnpm typecheck`. Expected: all green,
  `src/domain/Layout.ts` and `JobGroup.ts` written.
- [ ] **Step 5: Bump** to 0.13.0 (three files), add `layout` (`layout.json`: order and groups of the sidebar,
  reconciled with `jobs.json`) to the module list in `CLAUDE.md`.
- [ ] **Step 6: Commit** `feat: keep the sidebar's order and groups in layout.json`

### Task 2: Pure layout functions (front end)

**Files:**
- Create: `src/layout.ts`, `src/layout.test.ts`

**Interfaces:**
- Consumes: `Layout`, `JobGroup` from `src/domain`; `Tone`, `jobTone` from `src/jobs.ts`.
- Produces (all pure, returning a new `Layout`):
  ```ts
  export const UNGROUPED = 'ungrouped'                       // container id of the jobs without a group
  export function reconcile(layout: Layout | null, jobIds: string[]): Layout   // same rules as Rust
  export function containerOf(layout: Layout, jobId: string): string | null    // UNGROUPED, a group id, or null
  export function moveJob(layout: Layout, jobId: string, container: string, index: number): Layout
  export function moveGroup(layout: Layout, groupId: string, index: number): Layout
  export function addGroup(layout: Layout, id: string, name: string): Layout   // appended at the end, expanded
  export function renameGroup(layout: Layout, id: string, name: string): Layout // blank name → unchanged
  export function removeGroup(layout: Layout, id: string): Layout              // its jobs appended to ungrouped
  export function toggleGroup(layout: Layout, id: string): Layout
  export function groupTone(jobs: JobView[]): Tone   // worst first: bad, warn, running, ok, muted; empty → 'muted'
  export function newGroupId(): string               // `g-${Date.now().toString(36)}${random suffix}`
  ```
  `moveJob` takes the job out of its current list, then inserts it into `container` at `index`, clamped to the list's
  length. `index` is the position in the target list after removal. An unknown container leaves the layout unchanged.

- [ ] **Step 1: Write the failing tests** in `src/layout.test.ts`, one `it` per behaviour:
  - reconcile: null layout → all ungrouped in order; unknown ids dropped; new ids appended; duplicates keep the first place.
  - moveJob: reorder within ungrouped (`["a","b","c"]`, move `c` to 0 → `["c","a","b"]`); ungrouped → group at index;
    group → ungrouped; into an **empty** group; index past the end → appended; unknown container → unchanged.
  - moveGroup: `[g1,g2,g3]`, move `g3` to 0 → `[g3,g1,g2]`.
  - addGroup appends `{ id, name, collapsed: false, jobs: [] }`.
  - renameGroup trims; `'  '` keeps the old name.
  - removeGroup: ungrouped `["a"]`, group `["b","c"]` → ungrouped `["a","b","c"]`, group gone.
  - toggleGroup flips `collapsed`.
  - groupTone: `[ok, bad, running]` → `'bad'`; `[ok, running]` → `'running'`; `[]` → `'muted'`.
- [ ] **Step 2: Run** `pnpm test src/layout.test.ts`. Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `src/layout.ts`.
- [ ] **Step 4: Run** `pnpm test src/layout.test.ts && pnpm typecheck && pnpm lint`. Expected: PASS.
- [ ] **Step 5: Commit** `feat: move jobs and groups in the sidebar layout`

### Task 3: Groups in the sidebar (no drag yet)

**Files:**
- Modify: `src/components/Sidebar.tsx`, `src/components/Sidebar.test.tsx`, `src/App.tsx`, `src/styles.css`
- Create: `src/components/JobGroup.tsx` (a group's header and its rows) if `Sidebar.tsx` grows too much

**Interfaces:**
- Consumes: Task 1 `api.getLayout/saveLayout`; Task 2 functions.
- Produces: new `Sidebar` props `layout: Layout | null` and `onLayout: (layout: Layout) => void`. `Sidebar` renders
  `reconcile(layout, jobs.map(j => j.id))`, so it is always complete, and calls `onLayout` with every change.
  The job row button becomes an exported `JobRow({ job, selected, checks, onSelect })` for Task 4 to wrap.

`App`: `const [layout, setLayout] = useState<Layout | null>(null)`. Load it with `api.getLayout()` at start and
whenever the **set** of job ids changes (a sorted joined string in `useEffect` deps), not on every poll. An error
goes to `setBanner(errorsOf(e))` and the layout stays `null`, which renders every job ungrouped. `onLayout(next)`:
`setLayout(next)`, then `api.saveLayout(next).then(setLayout, e => { setBanner(errorsOf(e)); reload layout })`.

Sidebar look (see the spec's sketch): the ungrouped rows first, then each group. A group header button holds a
chevron (`▾`/`▸`), the name, `· count`, and when collapsed a `dot dot-${groupTone(jobs)}`. Double-clicking the name
turns it into an input (autofocus, Enter or blur saves, Escape cancels). A "⋯" button on the header opens a small
menu with "Rename" and "Delete group" (no `confirm()`: the jobs are not deleted). An expanded empty group shows
"No jobs". A "New group" button (small, next to the `Jobs · N` label) adds a group named "New group" and opens it
in rename mode right away.

- [ ] **Step 1: Write the failing tests** in `Sidebar.test.tsx` (extend `sidebar()` with `layout` and `onLayout`):
  - `shows the jobs without a group first, then each group with its jobs`.
  - `shows every job ungrouped when there is no layout` (`layout: null`).
  - `collapses a group and shows its worst state` → click the header: `onLayout` gets `collapsed: true`. Rendered
    collapsed with a failed job: the rows are hidden, the header has `.dot-bad` and `· 2`.
  - `renames a group on double click`: type "Monitors" + Enter → `onLayout` with the name. Clear + Enter → no call
    with a blank name. Escape → no call.
  - `deletes a group and keeps its jobs` → menu "Delete group" → `onLayout` with the group gone and its jobs ungrouped.
  - `adds a group` → "New group" → `onLayout` with a new group named "New group" at the end, and an input in focus.
  - In an `App` test, if `App` has tests or fixtures that mock `api` (check `src/test/setup.ts`), otherwise in Sidebar:
    `keeps the layout when the jobs refresh`. Re-render with the same ids but new `JobView`s → the group order and
    `collapsed` stay the same.
- [ ] **Step 2: Run** `pnpm test src/components/Sidebar.test.tsx`. Expected: the new tests FAIL.
- [ ] **Step 3: Implement** Sidebar, group header, App wiring, CSS (indent rows in a group by a few px, header styled
  like `.lbl`, consistent with `.side-row`).
- [ ] **Step 4: Run** `pnpm test && pnpm typecheck && pnpm lint`. Expected: PASS, the existing Sidebar tests included.
- [ ] **Step 5: Commit** `feat: put jobs in groups that collapse in the sidebar`

### Task 4: Drag and drop

**Files:**
- Modify: `package.json`, `pnpm-lock.yaml` (`pnpm add @dnd-kit/core @dnd-kit/sortable @dnd-kit/utilities`),
  `src/components/Sidebar.tsx` (or `JobGroup.tsx`), `src/styles.css`

**Interfaces:**
- Consumes: Task 2 `moveJob`, `moveGroup`, `containerOf`, `UNGROUPED`; Task 3 `JobRow`, `onLayout`.

Structure: one `DndContext` (sensors: `PointerSensor` with `activationConstraint: { distance: 5 }` so a click still
selects, and `KeyboardSensor` with `sortableKeyboardCoordinates`). One `SortableContext` for the ungrouped jobs, one
per group for its jobs (`verticalListSortingStrategy`), and one for the groups themselves (ids prefixed `group:` so
they never collide with job ids). Each container also registers a `useDroppable` (id `container:<id>`) on its body,
and a collapsed group registers it on its header, so empty and collapsed groups accept a job. `onDragOver` moves a
job across containers with `moveJob` in **local** state, so it shows live. `onDragEnd` finishes the move (or
`moveGroup` for a group) and calls `onLayout` once. `onDragCancel` drops the local state. Use `closestCenter`
collision detection and a `DragOverlay` showing the dragged row. Disable everything when `editing` (the nav is
already `inert`).

- [ ] **Step 1: Install** the three packages. Run `pnpm typecheck`. Expected: PASS.
- [ ] **Step 2: Write the failing test** `can be moved with the keyboard`: focus a job's drag handle (or the row),
  press Space, ArrowDown, Space → `onLayout` called with the two jobs swapped. If jsdom cannot drive dnd-kit's keyboard
  sensor (no layout rects), drop this test and say so in the commit. The moves are covered by Task 2's tests.
- [ ] **Step 3: Run** it. Expected: FAIL.
- [ ] **Step 4: Implement** as above.
- [ ] **Step 5: Run** `pnpm test && pnpm typecheck && pnpm lint && pnpm test:rust`. Expected: all PASS.
- [ ] **Step 6: Check in the app** with `pnpm tauri dev`. Drag a job within the list, into a group, into an empty
  group, onto a collapsed group, and out of a group. Drag a group. A click still selects. Quit and relaunch: the order
  and the collapsed state are kept. Check `~/.claude-jobs/layout.json`. Do not touch the launchd files (dev build).
- [ ] **Step 7: Commit** `feat: drag jobs and groups to reorder them in the sidebar`
