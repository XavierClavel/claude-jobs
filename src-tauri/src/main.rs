// No console window on Windows in release; harmless on macOS.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use claude_jobs_lib::runs::Trigger;

fn main() {
    // `claude-jobs run <job>` is what launchd calls: one headless run, no window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("run") {
        let Some(job_id) = args.get(1) else {
            eprintln!("usage: claude-jobs run <job_id> [--trigger manual]");
            std::process::exit(2);
        };
        let manual = args.windows(2).any(|w| w[0] == "--trigger" && w[1] == "manual");
        let trigger = if manual { Trigger::Manual } else { Trigger::Schedule };
        std::process::exit(claude_jobs_lib::runner::run_job(job_id, trigger));
    }
    claude_jobs_lib::run();
}
