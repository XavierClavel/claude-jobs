// No console window on Windows in release; harmless on macOS.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use claude_jobs_lib::cli;
use claude_jobs_lib::runs::Trigger;

fn main() {
    // `claude-jobs run <job>` is what launchd calls: one headless run, no window, and `poll` the events check. `status`,
    // `start`, `watch`, `save` and `repos` are for other apps, such as Dooock's widget, and for agents.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match (args.first().map(String::as_str), args.get(1)) {
        (Some("status"), _) => std::process::exit(cli::status()),
        (Some("start"), Some(job_id)) => std::process::exit(cli::start(job_id)),
        (Some("watch"), Some(job_id)) => std::process::exit(cli::watch(job_id)),
        (Some("save"), _) => std::process::exit(cli::save()),
        (Some("repos"), _) => std::process::exit(cli::repos()),
        (Some(command @ ("start" | "watch")), None) => {
            eprintln!("usage: claude-jobs {command} <job_id>");
            std::process::exit(2);
        }
        _ => {}
    }
    // What the events agent calls every minute: see `events`.
    if args.first().map(String::as_str) == Some("poll") {
        std::process::exit(claude_jobs_lib::events::poll());
    }
    if args.first().map(String::as_str) == Some("catch-up") {
        std::process::exit(claude_jobs_lib::runner::catch_up());
    }
    if args.first().map(String::as_str) == Some("run") {
        let Some(job_id) = args.get(1) else {
            eprintln!(
                "usage: claude-jobs run <job_id> [--trigger manual|catch-up|pull-request] [--pull-request <json>]"
            );
            std::process::exit(2);
        };
        let flag = |name: &str| args.windows(2).find(|w| w[0] == name).map(|w| w[1].as_str());
        let trigger = match flag("--trigger") {
            Some("manual") => Trigger::Manual,
            Some("catch-up") => Trigger::CatchUp,
            Some("pull-request") => Trigger::PullRequest,
            _ => Trigger::Schedule,
        };
        let pull_request = match flag("--pull-request").map(serde_json::from_str) {
            None => None,
            Some(Ok(pr)) => Some(pr),
            Some(Err(e)) => {
                eprintln!("--pull-request does not parse: {e}");
                std::process::exit(2);
            }
        };
        std::process::exit(claude_jobs_lib::runner::run_job(job_id, trigger, pull_request));
    }
    claude_jobs_lib::run();
}
