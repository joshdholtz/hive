use hive::projects::{add_project, load_projects, remove_project};

#[test]
fn projects_add_and_remove() {
    let base = std::env::temp_dir().join(format!("hive-projects-{}", std::process::id()));
    let hive_home = base.join("home");
    let project_dir = base.join("repo");

    std::fs::create_dir_all(&project_dir).unwrap();
    std::env::set_var("HIVE_HOME", &hive_home);

    let projects = add_project(&project_dir, Some("demo".to_string())).unwrap();
    assert_eq!(projects.projects.len(), 1);
    assert_eq!(projects.projects[0].name, "demo");

    let loaded = load_projects().unwrap();
    assert_eq!(loaded.projects.len(), 1);

    let removed = remove_project(&project_dir).unwrap();
    assert!(removed.projects.is_empty());

    let _ = std::fs::remove_dir_all(&base);
}
