// No console window on Windows in release; harmless on macOS.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use claude_jobs_lib::runs::Trigger;

fn main() {
    // `claude-jobs run <job>` is what launchd calls: one headless run, no window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("catch-up") {
        std::process::exit(claude_jobs_lib::runner::catch_up());
    }
    if args.first().map(String::as_str) == Some("run") {
        let Some(job_id) = args.get(1) else {
            eprintln!("usage: claude-jobs run <job_id> [--trigger manual|catch-up]");
            std::process::exit(2);
        };
        let trigger = match args.windows(2).find(|w| w[0] == "--trigger").map(|w| w[1].as_str()) {
            Some("manual") => Trigger::Manual,
            Some("catch-up") => Trigger::CatchUp,
            _ => Trigger::Schedule,
        };
        std::process::exit(claude_jobs_lib::runner::run_job(job_id, trigger));
    }
    claude_jobs_lib::run();
}
