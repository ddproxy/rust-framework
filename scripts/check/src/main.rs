use serde_yaml::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const LICENSES: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-3-Clause",
    "BSL-1.0",
    "CDLA-Permissive-2.0",
    "ISC",
    "Unicode-3.0",
    "Unlicense",
    "Zlib",
];

const REQUIRED_CHECKS: &[&str] = &["test", "Audit", "CodeQL", "dependency-review"];

fn fail(message: impl AsRef<str>) -> ! {
    eprintln!("FAIL: {}", message.as_ref());
    std::process::exit(1);
}

fn read_yaml(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
    serde_yaml::from_str(&text)
        .unwrap_or_else(|e| fail(format!("{}: invalid YAML: {e}", path.display())))
}

fn as_str(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str)
}

fn expected_jobs(stem: &str) -> &'static [&'static str] {
    match stem {
        "ci" => &["test"],
        "audit" => &["Audit"],
        "codeql" => &["analyze"],
        "dependency-review" => &["dependency-review"],
        "scorecard" => &["analysis"],
        "release" => &["create", "build", "publish", "attest"],
        other => fail(format!("unexpected workflow: {other}")),
    }
}

fn expected_permissions(stem: &str, job: &str) -> Vec<(&'static str, &'static str)> {
    match (stem, job) {
        ("ci", "test") | ("audit", "Audit") | ("dependency-review", "dependency-review") => {
            vec![("contents", "read")]
        }
        ("codeql", "analyze") => vec![("contents", "read"), ("security-events", "write")],
        ("scorecard", "analysis") => vec![
            ("contents", "read"),
            ("id-token", "write"),
            ("security-events", "write"),
        ],
        ("release", "create") | ("release", "publish") => vec![("contents", "write")],
        ("release", "build") => vec![("contents", "read")],
        ("release", "attest") => vec![
            ("contents", "write"),
            ("id-token", "write"),
            ("attestations", "write"),
        ],
        other => fail(format!("unexpected job: {other:?}")),
    }
}

fn check_pins(path: &Path) {
    let text = fs::read_to_string(path).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let Some(uses) = trimmed
            .strip_prefix("uses:")
            .or_else(|| trimmed.strip_prefix("- uses:"))
        else {
            continue;
        };
        let (action, comment) = match uses.split_once('#') {
            Some((action, comment)) => (action.trim(), Some(comment.trim())),
            None => (uses.trim(), None),
        };
        let Some((_, sha)) = action.rsplit_once('@') else {
            fail(format!("{}:{}: uses is not pinned: {action}", path.display(), line_number + 1));
        };
        if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            fail(format!("{}:{}: not a 40-character SHA: {sha}", path.display(), line_number + 1));
        }
        if !comment.is_some_and(|comment| comment.starts_with('v')) {
            fail(format!("{}:{}: missing version comment: {action}", path.display(), line_number + 1));
        }
    }
}

fn check_scorecard_restrictions(path: &Path, workflow: &Value) {
    if workflow.get("env").is_some() || workflow.get("defaults").is_some() {
        fail(format!("{}: scorecard forbids workflow env and defaults", path.display()));
    }
    let Some(job) = workflow
        .get("jobs")
        .and_then(Value::as_mapping)
        .and_then(|jobs| jobs.get(Value::String("analysis".into())))
    else {
        fail(format!("{}: missing scorecard job", path.display()));
    };
    if job.get("env").is_some() || job.get("defaults").is_some() {
        fail(format!("{}: scorecard forbids job env and defaults", path.display()));
    }
    let approved = [
        "actions/checkout",
        "actions/upload-artifact",
        "github/codeql-action/upload-sarif",
        "ossf/scorecard-action",
        "step-security/harden-runner",
    ];
    let Some(steps) = job.get("steps").and_then(Value::as_sequence) else {
        fail(format!("{}: scorecard job has no steps", path.display()));
    };
    for step in steps {
        if step.get("run").is_some() {
            fail(format!("{}: scorecard job cannot use run steps", path.display()));
        }
        let Some(uses) = as_str(step.get("uses")) else {
            fail(format!("{}: scorecard job step must be an approved action", path.display()));
        };
        let repo = uses.split('@').next().unwrap_or(uses);
        if !approved.contains(&repo) {
            fail(format!("{}: unapproved scorecard step: {repo}", path.display()));
        }
    }
}

fn check_dependabot(root: &Path) {
    let path = root.join(".github/dependabot.yml");
    let doc = read_yaml(&path);
    let updates = doc
        .get("updates")
        .and_then(Value::as_sequence)
        .unwrap_or_else(|| fail("dependabot.yml: no updates"));
    let mut ecosystems = BTreeSet::new();
    for update in updates {
        let ecosystem = as_str(update.get("package-ecosystem"))
            .unwrap_or_else(|| fail("dependabot.yml: missing package-ecosystem"));
        ecosystems.insert(ecosystem.to_owned());
        if as_str(update.get("schedule").and_then(|s| s.get("interval"))) != Some("weekly") {
            fail(format!("dependabot.yml: {ecosystem} is not weekly"));
        }
        if update.get("open-pull-requests-limit").and_then(Value::as_i64) != Some(5) {
            fail(format!("dependabot.yml: {ecosystem} limit is not 5"));
        }
        let groups = update
            .get("groups")
            .and_then(Value::as_mapping)
            .unwrap_or_else(|| fail(format!("dependabot.yml: {ecosystem} has no groups")));
        for (name, group) in groups {
            let types = group
                .get("update-types")
                .and_then(Value::as_sequence)
                .unwrap_or_else(|| fail(format!("dependabot.yml: group {name:?} has no update-types")));
            let types: BTreeSet<&str> = types.iter().filter_map(Value::as_str).collect();
            if !types.contains("minor") || !types.contains("patch") {
                fail(format!("dependabot.yml: group {name:?} is not minor and patch"));
            }
        }
    }
    for expected in ["cargo", "github-actions"] {
        if !ecosystems.contains(expected) {
            fail(format!("dependabot.yml: missing {expected}"));
        }
    }
}

fn check_policy(root: &Path, workflow: &Value) {
    let deny_path = root.join("deny.toml");
    let deny_text = fs::read_to_string(&deny_path)
        .unwrap_or_else(|e| fail(format!("{}: {e}", deny_path.display())));
    let deny: toml::Value = toml::from_str(&deny_text)
        .unwrap_or_else(|e| fail(format!("{}: invalid TOML: {e}", deny_path.display())));

    let deny_allow: BTreeSet<String> = deny
        .get("licenses")
        .and_then(|l| l.get("allow"))
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| fail("deny.toml: licenses.allow is missing"))
        .iter()
        .filter_map(toml::Value::as_str)
        .map(str::to_owned)
        .collect();
    let from_workflow = as_str(
        workflow
            .get("jobs")
            .and_then(Value::as_mapping)
            .and_then(|jobs| jobs.get(Value::String("dependency-review".into())))
            .and_then(|job| job.get("steps"))
            .and_then(Value::as_sequence)
            .and_then(|steps| steps.first())
            .and_then(|step| step.get("with"))
            .and_then(|with| with.get("allow-licenses")),
    )
    .unwrap_or_else(|| fail("dependency-review.yml: allow-licenses is missing"));
    let review_allow: BTreeSet<String> = from_workflow
        .split(',')
        .map(str::trim)
        .filter(|license| !license.is_empty())
        .map(str::to_owned)
        .collect();
    let expected: BTreeSet<String> = LICENSES.iter().map(|license| (*license).to_owned()).collect();
    if deny_allow != expected || review_allow != expected {
        fail(format!(
            "license allow lists differ: deny.toml={deny_allow:?}, dependency-review={review_allow:?}, expected={expected:?}"
        ));
    }

    let sources = deny.get("sources").unwrap_or_else(|| fail("deny.toml: sources is missing"));
    for key in ["unknown-registry", "unknown-git"] {
        if sources.get(key).and_then(toml::Value::as_str) != Some("deny") {
            fail(format!("deny.toml: sources.{key} must be deny"));
        }
    }
    if deny
        .get("advisories")
        .and_then(|a| a.get("yanked"))
        .and_then(toml::Value::as_str)
        != Some("deny")
    {
        fail("deny.toml: advisories.yanked must be deny");
    }
    let registries = sources
        .get("allow-registry")
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| fail("deny.toml: sources.allow-registry is missing"));
    if !registries
        .iter()
        .filter_map(toml::Value::as_str)
        .any(|url| url == "https://github.com/rust-lang/crates.io-index")
    {
        fail("deny.toml: crates.io index is not allowed");
    }
    let orgs = sources
        .get("allow-org")
        .and_then(|o| o.get("github"))
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| fail("deny.toml: sources.allow-org.github is missing"));
    if !orgs.iter().filter_map(toml::Value::as_str).any(|org| org == "ddproxy") {
        fail("deny.toml: github.com/ddproxy is not allowed");
    }
}

fn main() {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| ".".to_owned()),
    );
    let workflow_dir = root.join(".github/workflows");
    let mut paths: Vec<PathBuf> = fs::read_dir(&workflow_dir)
        .unwrap_or_else(|e| fail(format!("{}: {e}", workflow_dir.display())))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| matches!(path.extension().and_then(|e| e.to_str()), Some("yml" | "yaml")))
        .collect();
    paths.sort();
    if paths.is_empty() {
        fail("no workflow files found");
    }

    let mut check_names: BTreeSet<String> = BTreeSet::new();
    let mut review_workflow = None;
    for path in &paths {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let workflow = read_yaml(path);
        check_pins(path);

        if workflow.get("permissions").and_then(Value::as_mapping).is_none_or(|p| !p.is_empty()) {
            fail(format!("{}: top-level permissions must be {{}}", path.display()));
        }
        if stem == "scorecard" {
            check_scorecard_restrictions(path, &workflow);
        }
        if stem == "dependency-review" {
            review_workflow = Some(workflow.clone());
        }

        let jobs = workflow
            .get("jobs")
            .and_then(Value::as_mapping)
            .unwrap_or_else(|| fail(format!("{}: jobs is missing", path.display())));
        let expected_jobs = expected_jobs(stem);
        let mut actual_jobs: BTreeSet<String> = BTreeSet::new();
        for (key, job) in jobs {
            let name = key.as_str().unwrap_or_else(|| fail(format!("{}: non-string job key", path.display())));
            actual_jobs.insert(name.to_owned());
            check_names.insert(name.to_owned());
            if let Some(display_name) = as_str(job.get("name")) {
                check_names.insert(display_name.to_owned());
            }
            if !expected_jobs.contains(&name) {
                fail(format!("{}: unexpected job {name}", path.display()));
            }
            let expected: BTreeMap<&str, &str> = expected_permissions(stem, name).into_iter().collect();
            let granted: BTreeMap<String, String> = job
                .get("permissions")
                .and_then(Value::as_mapping)
                .unwrap_or_else(|| fail(format!("{}: job {name} has no permissions", path.display())))
                .iter()
                .map(|(key, value)| {
                    (
                        key.as_str().unwrap_or("").to_owned(),
                        as_str(Some(value)).unwrap_or("").to_owned(),
                    )
                })
                .collect();
            let expected_owned: BTreeMap<String, String> = expected
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect();
            if granted != expected_owned {
                fail(format!(
                    "{}: job {name} permissions {granted:?} != {expected_owned:?}",
                    path.display()
                ));
            }
        }
        if actual_jobs != expected_jobs.iter().map(|j| (*j).to_owned()).collect() {
            fail(format!("{}: job set does not match the expected checks", path.display()));
        }
    }
    for name in REQUIRED_CHECKS {
        if !check_names.contains(*name) {
            fail(format!("required check name {name} is missing"));
        }
    }

    check_dependabot(&root);
    check_policy(&root, review_workflow.as_ref().expect("dependency-review workflow parsed"));

    let readme = fs::read_to_string(root.join("README.md")).unwrap_or_else(|e| fail(format!("README.md: {e}")));
    if !readme.contains("api.securityscorecards.dev/projects/github.com/ddproxy/rust-framework/badge")
        || !readme.contains("docs/security-scanning.md")
    {
        fail("README.md: missing Scorecard badge or scanning link");
    }
    if readme.contains("crates.io") {
        fail("README.md: the crate is not published; no crates.io badge");
    }
    let nav = fs::read_to_string(root.join("mkdocs.yml")).unwrap_or_else(|e| fail(format!("mkdocs.yml: {e}")));
    if !nav.contains("security-scanning.md") {
        fail("mkdocs.yml: security-scanning.md is not in the navigation");
    }

    println!(
        "OK: {} workflows parse; SHA pins, permissions, required checks, dependabot, policy and docs agree",
        paths.len()
    );
}
