//! End-to-end checks that an empty or relative `OO_CONFIG_DIR` / `OO_DATA_DIR`
//! never makes files in the current directory act as oo's configuration, status
//! or database. Each case runs the `oo` binary from a temp directory that holds
//! repository-style files, so a cwd-relative resolution would be visible.
//!
//! Every child process also gets a temp `HOME` and XDG base directories, and
//! `OO_CONFIG_DIR` / `OO_DATA_DIR` are removed unless a test sets them. An
//! ignored value falls back to the platform default, and without this the
//! fallback would point at the developer's real config directory, where `oo`
//! consumes pending learn-status messages on every run.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

const STATUS_LINE: &str = "FAILED repo-fake: planted by the repository\n";
const PLANTED_PATTERN: &str = "^planted-marker-xyz";

/// Values that oo must ignore for both variables: empty, and relative.
const IGNORED_VALUES: [&str; 2] = ["", "relative-dir"];

/// Writes the files a repository could ship under `dir`: a config file with an
/// invalid retention (warns when indexing), a pending learn-status line and a
/// user pattern (listed by `oo patterns`).
fn plant_oo_files(dir: &Path) {
    std::fs::create_dir_all(dir.join("patterns")).unwrap();
    std::fs::write(dir.join("config.toml"), "[store]\nretention = -1\n").unwrap();
    std::fs::write(dir.join("learn-status.log"), STATUS_LINE).unwrap();
    std::fs::write(
        dir.join("patterns").join("planted.toml"),
        format!("command_match = \"{PLANTED_PATTERN}\"\n"),
    )
    .unwrap();
}

/// An `oo` command whose platform directories all point inside `home`.
fn oo(cwd: &Path, home: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("oo");
    cmd.current_dir(cwd)
        .env_remove("OO_CONFIG_DIR")
        .env_remove("OO_DATA_DIR")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("xdg-config"))
        .env("XDG_DATA_HOME", home.join("xdg-data"));
    cmd
}

/// Runs `oo seq 1 3000`, which reads the config and data directories in one run.
fn run_seq(
    cwd: &Path,
    home: &Path,
    config_dir: impl AsRef<OsStr>,
    data_dir: impl AsRef<OsStr>,
) -> Output {
    oo(cwd, home)
        .args(["seq", "1", "3000"])
        .env("OO_CONFIG_DIR", config_dir)
        .env("OO_DATA_DIR", data_dir)
        .output()
        .unwrap()
}

#[test]
fn ignored_config_dir_does_not_read_repo_files() {
    for value in IGNORED_VALUES {
        let repo = TempDir::new().unwrap();
        plant_oo_files(repo.path());
        // A relative value would be resolved against cwd, so plant there too.
        let resolved = repo.path().join(value);
        plant_oo_files(&resolved);
        let home = TempDir::new().unwrap();

        let patterns = oo(repo.path(), home.path())
            .arg("patterns")
            .env("OO_CONFIG_DIR", value)
            .output()
            .unwrap();
        assert!(patterns.status.success(), "OO_CONFIG_DIR={value:?}");
        assert!(
            !String::from_utf8_lossy(&patterns.stdout).contains(PLANTED_PATTERN),
            "OO_CONFIG_DIR={value:?}: planted user pattern was listed"
        );

        let seq = run_seq(repo.path(), home.path(), value, home.path().join("data"));
        assert!(seq.status.success(), "OO_CONFIG_DIR={value:?}");
        let stderr = String::from_utf8_lossy(&seq.stderr);
        assert!(!stderr.contains("planted by the repository"), "{stderr}");
        assert!(
            !stderr.contains("retention"),
            "OO_CONFIG_DIR={value:?}: planted config.toml was read: {stderr}"
        );
        for dir in [repo.path().to_path_buf(), resolved] {
            assert!(
                dir.join("learn-status.log").exists(),
                "OO_CONFIG_DIR={value:?}: {} must not be consumed",
                dir.display()
            );
        }
    }
}

#[test]
fn relative_config_dir_warns_once_per_process() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    // The warning is printed once at startup, so it does not depend on which
    // directories this command goes on to read.
    let out = run_seq(
        repo.path(),
        home.path(),
        "relative-cfg",
        home.path().join("data"),
    );
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr.matches("ignoring OO_CONFIG_DIR").count(),
        1,
        "{stderr}"
    );
}

#[test]
fn ignored_data_dir_does_not_create_database_in_cwd() {
    for value in IGNORED_VALUES {
        let repo = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let cfg = TempDir::new().unwrap();
        let out = run_seq(repo.path(), home.path(), cfg.path(), value);
        assert!(out.status.success(), "OO_DATA_DIR={value:?}");
        assert!(
            !repo.path().join("oo.db").exists(),
            "OO_DATA_DIR={value:?}: oo.db created in cwd"
        );
        assert!(
            !repo.path().join(value).join("oo.db").exists(),
            "OO_DATA_DIR={value:?}: oo.db created under cwd"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        let expected_warnings = usize::from(!value.is_empty());
        assert_eq!(
            stderr.matches("ignoring OO_DATA_DIR").count(),
            expected_warnings,
            "OO_DATA_DIR={value:?}: {stderr}"
        );
    }
}

#[test]
fn absolute_data_dir_still_holds_database() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let cfg = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let out = run_seq(repo.path(), home.path(), cfg.path(), data.path());
    assert!(out.status.success());
    assert!(data.path().join("oo.db").exists());
    assert!(!String::from_utf8_lossy(&out.stderr).contains("ignoring OO_DATA_DIR"));
}

/// Positive control for the negative cases above: the same planted layout is
/// read when it sits in a valid absolute `OO_CONFIG_DIR`.
#[test]
fn absolute_config_dir_with_planted_files_is_read() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let cfg = TempDir::new().unwrap();
    plant_oo_files(cfg.path());

    // `seq` runs first: every foreground run consumes learn-status.log, so a
    // prior `oo patterns` run would take the planted status line.
    let seq = run_seq(
        repo.path(),
        home.path(),
        cfg.path(),
        home.path().join("data"),
    );
    assert!(seq.status.success());
    let stderr = String::from_utf8_lossy(&seq.stderr);
    assert!(
        stderr.contains("retention"),
        "planted config.toml not read: {stderr}"
    );
    assert!(stderr.contains("planted by the repository"), "{stderr}");
    assert!(
        !cfg.path().join("learn-status.log").exists(),
        "planted learn-status.log was not consumed"
    );

    plant_oo_files(cfg.path());
    let patterns = oo(repo.path(), home.path())
        .arg("patterns")
        .env("OO_CONFIG_DIR", cfg.path())
        .output()
        .unwrap();
    assert!(patterns.status.success());
    assert!(
        String::from_utf8_lossy(&patterns.stdout).contains(PLANTED_PATTERN),
        "planted user pattern not listed for an absolute OO_CONFIG_DIR"
    );
}

/// A padded `OO_DATA_DIR` is ignored with a warning naming the variable. No
/// database appears at the padded location, the trimmed one, or under cwd.
#[test]
fn padded_data_dir_is_ignored_and_creates_no_database() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let cfg = TempDir::new().unwrap();
    let trimmed = home.path().join("data");
    let padded_values = [
        format!(" {}", trimmed.display()),
        format!("{} ", trimmed.display()),
    ];
    for padded in &padded_values {
        let out = run_seq(repo.path(), home.path(), cfg.path(), padded);
        assert!(out.status.success(), "OO_DATA_DIR={padded:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            stderr.matches("ignoring OO_DATA_DIR").count(),
            1,
            "OO_DATA_DIR={padded:?}: {stderr}"
        );
        // Path::join replaces the base for an absolute argument, so this covers
        // the padded absolute path (trailing space) and the cwd-relative one
        // (leading space).
        assert!(
            !repo.path().join(padded).join("oo.db").exists(),
            "OO_DATA_DIR={padded:?}: oo.db created at the padded location"
        );
        assert!(
            !trimmed.join("oo.db").exists(),
            "OO_DATA_DIR={padded:?}: oo.db created at the trimmed path"
        );
    }
}
