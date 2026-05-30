fn main() -> anyhow::Result<()> {
    let proj = std::path::PathBuf::from("/tmp/orrchdb_smoke");
    let conn = orrch_db::rebuild_all(
        &std::path::PathBuf::from("/tmp/orrchdb_smoke_db/orrch.db"),
        &orrch_db::RebuildSources {
            project_dirs: vec![proj],
            library_root: std::path::PathBuf::from("/tmp/orrchdb_smoke_lib"),
        },
    )?;
    let bugs = orrch_db::query::open_bugs(&conn, "orrchdb_smoke")?;
    println!("open bugs: {bugs:#?}");
    assert_eq!(bugs.len(), 1, "expected the seeded smoke bug");
    println!("OK: query observed the seeded event");
    Ok(())
}
