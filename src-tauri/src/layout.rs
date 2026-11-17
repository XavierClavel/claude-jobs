//! How the sidebar lays the jobs out: their order, and the groups they are put in. Stored in
//! `~/.claude-jobs/layout.json`, apart from `jobs.json`, which stays as other tools know it. It is display only, so
//! it is reconciled with the jobs on every read and every write: a job it does not know goes at the bottom of the
//! jobs without a group, and one that no longer exists is dropped.

use crate::paths;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use ts_rs::TS;

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[ts(export)]
pub struct Layout {
    /// The jobs in no group, shown first, in this order.
    #[serde(default)]
    pub ungrouped: Vec<String>,
    #[serde(default)]
    pub groups: Vec<JobGroup>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct JobGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub jobs: Vec<String>,
}

pub fn parse(text: &str) -> Result<Layout, String> {
    serde_json::from_str(text).map_err(|e| format!("layout.json does not parse: {e}"))
}

/// No file is every job in no group, which is how the sidebar showed them before there were groups.
pub fn load() -> Result<Layout, String> {
    match fs::read_to_string(paths::layout_file()) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Layout::default()),
        Err(e) => Err(format!("cannot read layout.json: {e}")),
    }
}

pub fn save(layout: &Layout) -> Result<(), String> {
    fs::create_dir_all(paths::base()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(layout).map_err(|e| e.to_string())?;
    let tmp = paths::layout_file().with_extension("tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, paths::layout_file()).map_err(|e| e.to_string())
}

/// `layout` with only the jobs of `job_ids`, each once, in its first place, and the ones it did not place at the
/// bottom of the jobs in no group, in the order of `job_ids`.
pub fn reconcile(layout: Layout, job_ids: &[String]) -> Layout {
    let exists: HashSet<&str> = job_ids.iter().map(String::as_str).collect();
    let mut placed: HashSet<String> = HashSet::new();
    let mut keep = |list: Vec<String>| -> Vec<String> {
        list.into_iter().filter(|id| exists.contains(id.as_str()) && placed.insert(id.clone())).collect()
    };
    let mut ungrouped = keep(layout.ungrouped);
    let mut group_ids = HashSet::new();
    let groups: Vec<JobGroup> = layout
        .groups
        .into_iter()
        .filter(|g| group_ids.insert(g.id.clone()))
        .map(|g| JobGroup { jobs: keep(g.jobs), ..g })
        .collect();
    ungrouped.extend(job_ids.iter().filter(|id| !placed.contains(*id)).cloned());
    Layout { ungrouped, groups }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn group(id: &str, jobs: &[&str]) -> JobGroup {
        JobGroup { id: id.into(), name: id.to_uppercase(), collapsed: false, jobs: ids(jobs) }
    }

    #[test]
    fn reconcile_with_no_layout_lists_every_job_ungrouped_in_order() {
        let layout = reconcile(Layout::default(), &ids(&["a", "b"]));
        assert_eq!(layout, Layout { ungrouped: ids(&["a", "b"]), groups: vec![] });
    }

    #[test]
    fn reconcile_drops_jobs_that_no_longer_exist() {
        let layout = reconcile(Layout { ungrouped: vec![], groups: vec![group("g", &["a", "x"])] }, &ids(&["a"]));
        assert_eq!(layout.groups[0].jobs, ids(&["a"]));
    }

    #[test]
    fn reconcile_appends_new_jobs_ungrouped() {
        let layout =
            reconcile(Layout { ungrouped: ids(&["b"]), groups: vec![group("g", &["a"])] }, &ids(&["a", "b", "c"]));
        assert_eq!(layout.ungrouped, ids(&["b", "c"]));
        assert_eq!(layout.groups[0].jobs, ids(&["a"]));
    }

    #[test]
    fn reconcile_keeps_a_job_listed_twice_in_its_first_place() {
        let layout = reconcile(Layout { ungrouped: ids(&["a"]), groups: vec![group("g", &["a"])] }, &ids(&["a"]));
        assert_eq!(layout.ungrouped, ids(&["a"]));
        assert!(layout.groups[0].jobs.is_empty());
    }

    #[test]
    fn reconcile_keeps_empty_groups_and_their_state() {
        let empty = JobGroup { collapsed: true, ..group("g", &[]) };
        let layout = reconcile(Layout { ungrouped: vec![], groups: vec![empty.clone()] }, &ids(&["a"]));
        assert_eq!(layout.groups, vec![empty]);
    }

    #[test]
    fn reconcile_drops_a_repeated_group_id() {
        let layout = reconcile(
            Layout { ungrouped: vec![], groups: vec![group("g", &["a"]), group("g", &["b"])] },
            &ids(&["a", "b"]),
        );
        assert_eq!(layout.groups, vec![group("g", &["a"])]);
        assert_eq!(layout.ungrouped, ids(&["b"]));
    }

    #[test]
    fn layout_parses_without_optional_fields() {
        let layout = parse(r#"{"groups":[{"id":"g","name":"G"}]}"#).unwrap();
        assert!(layout.ungrouped.is_empty());
        assert_eq!(layout.groups, vec![JobGroup { id: "g".into(), name: "G".into(), collapsed: false, jobs: vec![] }]);
    }

    #[test]
    fn parse_refuses_a_file_that_does_not_parse() {
        assert!(parse("{not json").unwrap_err().contains("does not parse"));
    }
}
