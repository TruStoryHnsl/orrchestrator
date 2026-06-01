//! Human-readable report of a Registry — the thing you LOOK AT to know the run worked.

use crate::registry::{Registry, Target, TargetStatus};

fn caps_summary(t: &Target) -> String {
    let c = &t.capabilities;
    let mut on = Vec::new();
    if c.camera { on.push("cam"); }
    if c.mic { on.push("mic"); }
    if c.gpu { on.push("gpu"); }
    if c.filesystem { on.push("fs"); }
    if c.public_web_host { on.push("web-host"); }
    if on.is_empty() { "-".into() } else { on.join("+") }
}

fn status_mark(s: TargetStatus) -> &'static str {
    match s {
        TargetStatus::Reachable => "OK  ✓",
        TargetStatus::Unreachable => "UNREACHABLE ✗",
        TargetStatus::ProbeFailed => "PROBE-FAILED ✗",
    }
}

/// Print the registry as a table; return how many targets are Reachable.
pub fn print(reg: &Registry) -> usize {
    println!("\n  orrdeal — skeleton run report");
    println!("  ─────────────────────────────────────────────────────────────────────");
    println!(
        "  {:<28} {:<10} {:<8} {:<12} STATUS",
        "TARGET", "OS", "ARCH", "CAPS"
    );
    let mut reachable = 0;
    for t in &reg.targets {
        if t.status == TargetStatus::Reachable {
            reachable += 1;
        }
        println!(
            "  {:<28} {:<10} {:<8} {:<12} {}",
            t.id,
            if t.os.is_empty() { "-" } else { &t.os },
            if t.arch.is_empty() { "-" } else { &t.arch },
            caps_summary(t),
            status_mark(t.status),
        );
        if !t.note.is_empty() {
            println!("      └─ {}", t.note);
        }
    }
    println!("  ─────────────────────────────────────────────────────────────────────");
    println!("  {reachable}/{} targets reachable\n", reg.targets.len());
    reachable
}
