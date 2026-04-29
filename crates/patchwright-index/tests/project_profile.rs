use patchwright_core::types::RepoPath;
use patchwright_index::profile_project;
use patchwright_test_support::TempRepo;

#[test]
fn profiles_rust_project_from_cargo_manifest() {
    let repo = TempRepo::new("profile-rust");
    repo.write("Cargo.toml", "[package]\nname = \"profile_rust\"\n");
    repo.write("src/lib.rs", "pub fn ok() {}\n");
    repo.write("tests/smoke.rs", "#[test]\nfn smoke() {}\n");
    repo.write(".github/workflows/ci.yml", "name: ci\n");

    let profile = profile_project(repo.root()).unwrap();

    assert_eq!(profile.detected_languages, vec!["Rust"]);
    assert_eq!(profile.manifests, vec![RepoPath::new("Cargo.toml")]);
    assert_eq!(profile.package_managers, vec!["cargo"]);
    assert!(profile
        .build_commands
        .iter()
        .any(|command| command == "cargo build"));
    assert!(profile
        .test_commands
        .iter()
        .any(|command| command == "cargo test"));
    assert_eq!(profile.source_roots, vec![RepoPath::new("src")]);
    assert_eq!(profile.test_roots, vec![RepoPath::new("tests")]);
    assert_eq!(
        profile.ci_files,
        vec![RepoPath::new(".github/workflows/ci.yml")]
    );
}

#[test]
fn profiles_typescript_project_from_package_and_tsconfig() {
    let repo = TempRepo::new("profile-typescript");
    repo.write("package.json", "{\"scripts\":{\"test\":\"vitest\"}}\n");
    repo.write("pnpm-lock.yaml", "lockfileVersion: '9.0'\n");
    repo.write("tsconfig.json", "{}\n");
    repo.write("src/index.ts", "export const ok = true;\n");
    repo.write("app/api/users/route.ts", "export function GET() {}\n");
    repo.write("__tests__/index.test.ts", "test('ok', () => {});\n");

    let profile = profile_project(repo.root()).unwrap();

    assert_eq!(profile.detected_languages, vec!["TypeScript", "JavaScript"]);
    assert_eq!(
        profile.manifests,
        vec![
            RepoPath::new("package.json"),
            RepoPath::new("pnpm-lock.yaml"),
            RepoPath::new("tsconfig.json")
        ]
    );
    assert_eq!(profile.package_managers, vec!["pnpm"]);
    assert!(profile
        .build_commands
        .iter()
        .any(|command| command == "pnpm build"));
    assert!(profile
        .typecheck_commands
        .iter()
        .any(|command| command == "pnpm tsc --noEmit"));
    assert!(profile.source_roots.contains(&RepoPath::new("app")));
    assert!(profile.source_roots.contains(&RepoPath::new("src")));
    assert_eq!(profile.test_roots, vec![RepoPath::new("__tests__")]);
}

#[test]
fn profiles_python_project_from_pyproject_and_requirements() {
    let repo = TempRepo::new("profile-python");
    repo.write("pyproject.toml", "[project]\nname = \"profile-python\"\n");
    repo.write("requirements.txt", "pytest\n");
    repo.write("app/main.py", "def ok():\n    return True\n");
    repo.write("tests/test_main.py", "def test_ok():\n    assert True\n");

    let profile = profile_project(repo.root()).unwrap();

    assert_eq!(profile.detected_languages, vec!["Python"]);
    assert_eq!(
        profile.manifests,
        vec![
            RepoPath::new("pyproject.toml"),
            RepoPath::new("requirements.txt")
        ]
    );
    assert_eq!(profile.package_managers, vec!["pip"]);
    assert!(profile
        .test_commands
        .iter()
        .any(|command| command == "python -m pytest"));
    assert_eq!(profile.source_roots, vec![RepoPath::new("app")]);
    assert_eq!(profile.test_roots, vec![RepoPath::new("tests")]);
}
