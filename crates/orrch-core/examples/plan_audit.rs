use orrch_core::plan_parser::{lint_plan, parse_plan};
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> std::io::Result<()> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let projects_root = Path::new(&home).join("projects");
    let mut plans = Vec::new();
    collect_plans(&projects_root, &mut plans)?;
    plans.sort();

    for plan_path in plans {
        let content = fs::read_to_string(&plan_path)?;
        let phases = parse_plan(&content);
        let feature_count: usize = phases.iter().map(|phase| phase.features.len()).sum();
        let project = project_name(&projects_root, &plan_path);

        println!(
            "{}: phases={}, features={}, path={}",
            project,
            phases.len(),
            feature_count,
            plan_path.display()
        );

        for warning in lint_plan(&content) {
            println!("  warning: {warning}");
        }
    }

    Ok(())
}

fn collect_plans(dir: &Path, plans: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let orrch_plan = dir.join(".orrch").join("PLAN.md");
    if orrch_plan.is_file() {
        plans.push(orrch_plan);
    } else {
        let plan = dir.join("PLAN.md");
        if plan.is_file() {
            plans.push(plan);
        }
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name();
        if name == ".git" || name == ".orrch" || name == ".claude" || name == "target" {
            continue;
        }

        collect_plans(&path, plans)?;
    }

    Ok(())
}

fn project_name(projects_root: &Path, plan_path: &Path) -> String {
    let project_dir = if plan_path.file_name().and_then(|name| name.to_str()) == Some("PLAN.md")
        && plan_path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            == Some(".orrch")
    {
        plan_path
            .parent()
            .and_then(|parent| parent.parent())
            .unwrap_or(plan_path)
    } else {
        plan_path.parent().unwrap_or(plan_path)
    };

    project_dir
        .strip_prefix(projects_root)
        .unwrap_or(project_dir)
        .display()
        .to_string()
}
