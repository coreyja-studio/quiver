use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

// `cargo_bin` is deprecated in assert_cmd 2.x in favor of `cargo_bin` on `Command`
// from the `CommandCargoPath` trait, but the free function form is simpler for tests.
#[allow(deprecated)]
fn quiver() -> Command {
    Command::cargo_bin("quiver").unwrap()
}

fn setup() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn config_path(dir: &TempDir) -> String {
    dir.path().join("config.toml").to_string_lossy().to_string()
}

fn create_skill(source_dir: &Path, name: &str) {
    let skill_dir = source_dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), format!("# {name}\n")).unwrap();
}

// --- init tests ---

#[test]
fn init_creates_config_with_defaults() {
    let tmp = setup();
    let cfg = config_path(&tmp);

    quiver()
        .args(["--config", &cfg, "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Created config"));

    let contents = fs::read_to_string(tmp.path().join("config.toml")).unwrap();
    assert!(contents.contains("~/.claude/skills"));
}

#[test]
fn init_with_config_flag_writes_to_specified_path() {
    let tmp = setup();
    let custom = tmp.path().join("sub/dir/my-config.toml");
    let cfg = custom.to_string_lossy().to_string();

    quiver().args(["--config", &cfg, "init"]).assert().success();

    assert!(custom.exists());
}

#[test]
fn init_is_idempotent() {
    let tmp = setup();
    let cfg = config_path(&tmp);

    quiver().args(["--config", &cfg, "init"]).assert().success();

    // Add a source to the config so we can verify it's preserved
    quiver()
        .args(["--config", &cfg, "add", "/some/path", "mysource"])
        .assert()
        .success();

    // Second init should NOT overwrite the config
    quiver()
        .args(["--config", &cfg, "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already exists"));

    // Verify the source we added is still present in the config
    let contents = fs::read_to_string(tmp.path().join("config.toml")).unwrap();
    assert!(
        contents.contains("mysource"),
        "Config was overwritten by second init — source 'mysource' is missing"
    );
    assert!(
        contents.contains("/some/path"),
        "Config was overwritten by second init — path '/some/path' is missing"
    );
}

// --- add tests ---

#[test]
fn add_appends_source_with_name() {
    let tmp = setup();
    let cfg = config_path(&tmp);

    quiver().args(["--config", &cfg, "init"]).assert().success();
    quiver()
        .args(["--config", &cfg, "add", "/some/path", "myname"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Added source 'myname'"));

    let contents = fs::read_to_string(tmp.path().join("config.toml")).unwrap();
    assert!(contents.contains("/some/path"));
    assert!(contents.contains("myname"));
}

#[test]
fn add_with_prefix_stores_custom_prefix() {
    let tmp = setup();
    let cfg = config_path(&tmp);

    quiver().args(["--config", &cfg, "init"]).assert().success();
    quiver()
        .args([
            "--config",
            &cfg,
            "add",
            "/some/path",
            "myname",
            "--prefix",
            "mp",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(tmp.path().join("config.toml")).unwrap();
    assert!(contents.contains("prefix = \"mp\""));
}

#[test]
fn add_same_path_twice_is_idempotent() {
    let tmp = setup();
    let cfg = config_path(&tmp);

    quiver().args(["--config", &cfg, "init"]).assert().success();
    quiver()
        .args(["--config", &cfg, "add", "/some/path", "first"])
        .assert()
        .success();
    quiver()
        .args(["--config", &cfg, "add", "/some/path", "second"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already exists"));
}

#[test]
fn add_same_name_twice_is_rejected() {
    let tmp = setup();
    let cfg = config_path(&tmp);

    quiver().args(["--config", &cfg, "init"]).assert().success();
    quiver()
        .args(["--config", &cfg, "add", "/path/a", "same"])
        .assert()
        .success();
    quiver()
        .args(["--config", &cfg, "add", "/path/b", "same"])
        .assert()
        .failure();
}

#[test]
fn add_trailing_slash_duplicate_detection() {
    let tmp = setup();
    let cfg = config_path(&tmp);

    quiver().args(["--config", &cfg, "init"]).assert().success();
    quiver()
        .args(["--config", &cfg, "add", "/some/path/", "first"])
        .assert()
        .success();
    quiver()
        .args(["--config", &cfg, "add", "/some/path", "second"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already exists"));
}

// --- sync tests ---

#[test]
fn sync_creates_symlinks_no_conflicts() {
    let tmp = setup();
    let cfg = config_path(&tmp);
    let target = tmp.path().join("target_skills");
    let src = tmp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    create_skill(&src, "my-skill");
    create_skill(&src, "other-skill");

    // Write config directly with absolute target
    let config_content = format!(
        "target = \"{}\"\n\n[[source]]\npath = \"{}\"\nname = \"personal\"\n",
        target.display(),
        src.display()
    );
    fs::write(tmp.path().join("config.toml"), config_content).unwrap();

    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Created: 2"));

    assert!(target.join("my-skill").is_symlink());
    assert!(target.join("other-skill").is_symlink());
}

#[test]
fn sync_prefixes_when_same_base_name_in_two_sources() {
    let tmp = setup();
    let cfg = config_path(&tmp);
    let target = tmp.path().join("target");
    let src1 = tmp.path().join("src1");
    let src2 = tmp.path().join("src2");

    fs::create_dir_all(&src1).unwrap();
    fs::create_dir_all(&src2).unwrap();
    create_skill(&src1, "bluesky");
    create_skill(&src2, "bluesky");

    let config_content = format!(
        "target = \"{}\"\n\n[[source]]\npath = \"{}\"\nname = \"personal\"\n\n[[source]]\npath = \"{}\"\nname = \"team\"\nprefix = \"tm\"\n",
        target.display(),
        src1.display(),
        src2.display()
    );
    fs::write(tmp.path().join("config.toml"), config_content).unwrap();

    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Created: 2"));

    // Both should be prefixed
    assert!(target.join("personal-bluesky").is_symlink());
    assert!(target.join("tm-bluesky").is_symlink());
    // Bare name should NOT exist
    assert!(!target.join("bluesky").exists());
}

#[test]
fn sync_skips_already_correct_symlinks() {
    let tmp = setup();
    let cfg = config_path(&tmp);
    let target = tmp.path().join("target");
    let src = tmp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    create_skill(&src, "my-skill");

    let config_content = format!(
        "target = \"{}\"\n\n[[source]]\npath = \"{}\"\nname = \"personal\"\n",
        target.display(),
        src.display()
    );
    fs::write(tmp.path().join("config.toml"), config_content).unwrap();

    // First sync creates
    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Created: 1"));

    // Second sync skips
    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Skipped: 1"));
}

#[test]
fn sync_warns_on_real_dirs_in_target() {
    let tmp = setup();
    let cfg = config_path(&tmp);
    let target = tmp.path().join("target");
    let src = tmp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    create_skill(&src, "my-skill");

    // Pre-create a real dir at the target location
    fs::create_dir_all(target.join("my-skill")).unwrap();

    let config_content = format!(
        "target = \"{}\"\n\n[[source]]\npath = \"{}\"\nname = \"personal\"\n",
        target.display(),
        src.display()
    );
    fs::write(tmp.path().join("config.toml"), config_content).unwrap();

    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Conflicts: 1"))
        .stderr(predicates::str::contains("not a symlink"));
}

#[test]
fn sync_prunes_stale_symlinks() {
    let tmp = setup();
    let cfg = config_path(&tmp);
    let target = tmp.path().join("target");
    let src = tmp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&target).unwrap();
    create_skill(&src, "my-skill");

    // Create a stale symlink
    let stale_target = tmp.path().join("stale-dir");
    fs::create_dir_all(&stale_target).unwrap();
    symlink(&stale_target, target.join("old-skill")).unwrap();

    let config_content = format!(
        "target = \"{}\"\n\n[[source]]\npath = \"{}\"\nname = \"personal\"\n",
        target.display(),
        src.display()
    );
    fs::write(tmp.path().join("config.toml"), config_content).unwrap();

    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Pruned: 1"))
        .stdout(predicates::str::contains("Created: 1"));

    assert!(!target.join("old-skill").exists());
    assert!(target.join("my-skill").is_symlink());
}

#[test]
fn sync_transitions_prefixed_to_unprefixed_when_conflict_removed() {
    let tmp = setup();
    let cfg = config_path(&tmp);
    let target = tmp.path().join("target");
    let src1 = tmp.path().join("src1");
    let src2 = tmp.path().join("src2");

    fs::create_dir_all(&src1).unwrap();
    fs::create_dir_all(&src2).unwrap();
    create_skill(&src1, "bluesky");
    create_skill(&src2, "bluesky");

    // Config with two sources that conflict on "bluesky"
    let config_content = format!(
        "target = \"{}\"\n\n[[source]]\npath = \"{}\"\nname = \"personal\"\n\n[[source]]\npath = \"{}\"\nname = \"team\"\n",
        target.display(),
        src1.display(),
        src2.display()
    );
    fs::write(tmp.path().join("config.toml"), &config_content).unwrap();

    // First sync: both sources conflict, so both get prefixed
    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Created: 2"));

    assert!(target.join("personal-bluesky").is_symlink());
    assert!(target.join("team-bluesky").is_symlink());
    assert!(!target.join("bluesky").exists());

    // Remove source2 from config — now only one source has "bluesky"
    let config_content = format!(
        "target = \"{}\"\n\n[[source]]\npath = \"{}\"\nname = \"personal\"\n",
        target.display(),
        src1.display()
    );
    fs::write(tmp.path().join("config.toml"), config_content).unwrap();

    // Second sync: no conflict, so "bluesky" should be unprefixed
    // Old prefixed symlinks should be pruned
    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Created: 1"))
        .stdout(predicates::str::contains("Pruned: 2"));

    // New unprefixed symlink exists
    assert!(target.join("bluesky").is_symlink());
    // Old prefixed symlinks are gone
    assert!(target.join("personal-bluesky").symlink_metadata().is_err());
    assert!(target.join("team-bluesky").symlink_metadata().is_err());
}

#[test]
fn sync_removes_dead_symlinks() {
    let tmp = setup();
    let cfg = config_path(&tmp);
    let target = tmp.path().join("target");
    let src = tmp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&target).unwrap();
    create_skill(&src, "my-skill");

    // Create a dead symlink (points to nonexistent)
    symlink("/nonexistent/path", target.join("dead-skill")).unwrap();

    let config_content = format!(
        "target = \"{}\"\n\n[[source]]\npath = \"{}\"\nname = \"personal\"\n",
        target.display(),
        src.display()
    );
    fs::write(tmp.path().join("config.toml"), config_content).unwrap();

    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Pruned: 1"));

    // Verify the dead symlink was fully removed (not just broken)
    assert!(
        target.join("dead-skill").symlink_metadata().is_err(),
        "dead symlink should have been pruned entirely"
    );
}

#[test]
fn sync_handles_prefix_collision() {
    let tmp = setup();
    let cfg = config_path(&tmp);
    let target = tmp.path().join("target");
    let src1 = tmp.path().join("src1");
    let src2 = tmp.path().join("src2");

    fs::create_dir_all(&src1).unwrap();
    fs::create_dir_all(&src2).unwrap();
    create_skill(&src1, "bluesky");
    create_skill(&src2, "bluesky");

    // Both sources have the SAME prefix, so there will be a collision
    let config_content = format!(
        "target = \"{}\"\n\n[[source]]\npath = \"{}\"\nname = \"a\"\nprefix = \"same\"\n\n[[source]]\npath = \"{}\"\nname = \"b\"\nprefix = \"same\"\n",
        target.display(),
        src1.display(),
        src2.display()
    );
    fs::write(tmp.path().join("config.toml"), config_content).unwrap();

    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Conflicts: 1"))
        .stderr(predicates::str::contains("prefix collision"));
}

// --- list tests ---

#[test]
fn list_shows_symlinks_with_targets_and_local_dirs() {
    let tmp = setup();
    let cfg = config_path(&tmp);
    let target = tmp.path().join("target");
    let src = tmp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    create_skill(&src, "linked-skill");

    // Create a local dir in target
    fs::create_dir_all(target.join("local-skill")).unwrap();

    let config_content = format!(
        "target = \"{}\"\n\n[[source]]\npath = \"{}\"\nname = \"personal\"\n",
        target.display(),
        src.display()
    );
    fs::write(tmp.path().join("config.toml"), config_content).unwrap();

    quiver().args(["--config", &cfg, "sync"]).assert().success();

    quiver()
        .args(["--config", &cfg, "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("linked-skill ->"))
        .stdout(predicates::str::contains("local-skill (local)"));
}

// --- error handling tests ---

#[test]
fn missing_source_dir_prints_warning() {
    let tmp = setup();
    let cfg = config_path(&tmp);
    let target = tmp.path().join("target");

    let config_content = format!(
        "target = \"{}\"\n\n[[source]]\npath = \"/nonexistent/path\"\nname = \"missing\"\n",
        target.display()
    );
    fs::write(tmp.path().join("config.toml"), config_content).unwrap();

    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .success()
        .stderr(predicates::str::contains("Warning: cannot access source"));
}

#[test]
fn missing_config_gives_helpful_error() {
    let tmp = setup();
    let cfg = tmp
        .path()
        .join("nonexistent.toml")
        .to_string_lossy()
        .to_string();

    quiver()
        .args(["--config", &cfg, "sync"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("quiver init"));
}
