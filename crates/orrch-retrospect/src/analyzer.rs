//! Retrospective analyzer — mines error stores across all projects
//! for recurring patterns, cross-project trends, and resolution effectiveness.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error_parser::ErrorCategory;
use crate::store::{ErrorRecord, ErrorStore};

/// Analysis results for a single project.
#[derive(Debug, Clone)]
pub struct ProjectAnalysis {
    pub project_name: String,
    pub project_dir: PathBuf,
    /// Error classes ranked by frequency.
    pub recurring_errors: Vec<ErrorClass>,
    /// Resolution rate: resolved / total unique errors.
    pub resolution_rate: f64,
    /// Average time to resolution (seconds), if available.
    pub avg_resolution_time: Option<f64>,
    /// Total error occurrences.
    pub total_occurrences: usize,
    /// Unique error fingerprints.
    pub unique_errors: usize,
}

/// A class of recurring errors (grouped by category + normalized pattern).
#[derive(Debug, Clone)]
pub struct ErrorClass {
    pub category: ErrorCategory,
    pub fingerprint: String,
    pub sample_context: String,
    pub occurrence_count: usize,
    pub resolved: bool,
    pub resolution: Option<String>,
    /// Projects that have seen this same fingerprint.
    pub seen_in_projects: Vec<String>,
}

/// Cross-project analysis results.
#[derive(Debug, Clone)]
pub struct EcosystemAnalysis {
    pub per_project: Vec<ProjectAnalysis>,
    /// Error classes that appear in 2+ projects.
    pub cross_project_patterns: Vec<ErrorClass>,
    /// Categories ranked by total frequency across all projects.
    pub top_categories: Vec<(ErrorCategory, usize)>,
    pub total_errors_ecosystem: usize,
    pub total_projects_scanned: usize,
}

/// Run a retrospective analysis across all projects in a directory.
pub fn analyze_ecosystem(projects_dir: &Path) -> EcosystemAnalysis {
    let mut per_project = Vec::new();
    let mut global_fp_map: HashMap<String, ErrorClass> = HashMap::new();
    let mut category_counts: HashMap<ErrorCategory, usize> = HashMap::new();
    let mut total_errors = 0;
    let mut projects_scanned = 0;

    let Ok(entries) = std::fs::read_dir(projects_dir) else {
        return empty_analysis();
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if name.starts_with('.') || name == "deprecated" { continue; }

        let retrospect_dir = path.join(".retrospect");
        if !retrospect_dir.exists() { continue; }

        projects_scanned += 1;
        let mut store = ErrorStore::new(&path);
        let stats = store.stats();

        if stats.total_occurrences == 0 { continue; }

        total_errors += stats.total_occurrences;

        // Load all records and group by fingerprint
        let records = load_all_records(&path);
        let mut fp_groups: HashMap<String, Vec<&ErrorRecord>> = HashMap::new();
        for rec in &records {
            fp_groups.entry(rec.fingerprint.clone()).or_default().push(rec);
        }

        let mut recurring = Vec::new();
        for (fp, recs) in &fp_groups {
            let first = recs[0];
            let resolved = recs.iter().any(|r| r.resolved);
            let resolution = recs.iter().rev().find(|r| r.resolved).and_then(|r| r.resolution.clone());

            *category_counts.entry(first.category).or_insert(0) += recs.len();

            let class = ErrorClass {
                category: first.category,
                fingerprint: fp.clone(),
                sample_context: first.raw_context.chars().take(200).collect(),
                occurrence_count: recs.len(),
                resolved,
                resolution: resolution.clone(),
                seen_in_projects: vec![name.clone()],
            };
            recurring.push(class.clone());

            // Track cross-project patterns
            let global = global_fp_map.entry(fp.clone()).or_insert_with(|| ErrorClass {
                category: first.category,
                fingerprint: fp.clone(),
                sample_context: first.raw_context.chars().take(200).collect(),
                occurrence_count: 0,
                resolved,
                resolution,
                seen_in_projects: Vec::new(),
            });
            global.occurrence_count += recs.len();
            if !global.seen_in_projects.contains(&name) {
                global.seen_in_projects.push(name.clone());
            }
        }

        recurring.sort_by(|a, b| b.occurrence_count.cmp(&a.occurrence_count));

        let resolution_rate = if stats.unique_errors > 0 {
            stats.resolved as f64 / stats.unique_errors as f64
        } else {
            0.0
        };

        // Compute average resolution time
        let mut resolution_times = Vec::new();
        for recs in fp_groups.values() {
            let first_time = recs.iter().map(|r| r.timestamp).fold(f64::MAX, f64::min);
            if let Some(res) = recs.iter().find(|r| r.resolved && r.resolution_timestamp.is_some()) {
                let rt = res.resolution_timestamp.unwrap() - first_time;
                if rt > 0.0 { resolution_times.push(rt); }
            }
        }
        let avg_resolution_time = if resolution_times.is_empty() {
            None
        } else {
            Some(resolution_times.iter().sum::<f64>() / resolution_times.len() as f64)
        };

        per_project.push(ProjectAnalysis {
            project_name: name,
            project_dir: path,
            recurring_errors: recurring,
            resolution_rate,
            avg_resolution_time,
            total_occurrences: stats.total_occurrences,
            unique_errors: stats.unique_errors,
        });
    }

    // Filter cross-project patterns: fingerprints seen in 2+ projects
    let cross_project: Vec<ErrorClass> = global_fp_map.into_values()
        .filter(|c| c.seen_in_projects.len() >= 2)
        .collect();

    // Sort categories by count
    let mut top_categories: Vec<(ErrorCategory, usize)> = category_counts.into_iter().collect();
    top_categories.sort_by(|a, b| b.1.cmp(&a.1));

    EcosystemAnalysis {
        per_project,
        cross_project_patterns: cross_project,
        top_categories,
        total_errors_ecosystem: total_errors,
        total_projects_scanned: projects_scanned,
    }
}

/// Load all error records from a project's JSONL store.
fn load_all_records(project_dir: &Path) -> Vec<ErrorRecord> {
    let store_path = project_dir.join(".retrospect").join("errors.jsonl");
    let Ok(contents) = std::fs::read_to_string(&store_path) else {
        return Vec::new();
    };
    contents.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<ErrorRecord>(l).ok())
        .collect()
}

fn empty_analysis() -> EcosystemAnalysis {
    EcosystemAnalysis {
        per_project: Vec::new(),
        cross_project_patterns: Vec::new(),
        top_categories: Vec::new(),
        total_errors_ecosystem: 0,
        total_projects_scanned: 0,
    }
}
