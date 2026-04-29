use patchwright_core::types::RepoPath;
use patchwright_index::architecture_map;
use patchwright_test_support::TempRepo;

#[test]
fn maps_rust_source_tests_and_manifest() {
    let repo = TempRepo::new("architecture-map-rust");
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"architecture_map_rust\"\n",
    );
    repo.write("src/lib.rs", "pub fn ok() {}\n");
    repo.write("tests/smoke.rs", "#[test]\nfn smoke() {}\n");

    let map = architecture_map(repo.root()).unwrap();

    assert_eq!(map.manifests, vec![RepoPath::new("Cargo.toml")]);
    assert_eq!(map.source_roots, vec![RepoPath::new("src")]);
    assert_eq!(map.test_roots, vec![RepoPath::new("tests")]);
}

#[test]
fn maps_typescript_routes_tests_and_config() {
    let repo = TempRepo::new("architecture-map-typescript");
    repo.write("package.json", "{}\n");
    repo.write("tsconfig.json", "{}\n");
    repo.write("app/api/users/route.ts", "export function GET() {}\n");
    repo.write("src/services/user_service.ts", "export const users = [];\n");
    repo.write("src/models/user.ts", "export type User = {};\n");
    repo.write("__tests__/users.test.ts", "test('users', () => {});\n");

    let map = architecture_map(repo.root()).unwrap();

    assert!(map
        .route_files
        .contains(&RepoPath::new("app/api/users/route.ts")));
    assert!(map
        .service_files
        .contains(&RepoPath::new("src/services/user_service.ts")));
    assert!(map
        .model_files
        .contains(&RepoPath::new("src/models/user.ts")));
    assert_eq!(map.test_roots, vec![RepoPath::new("__tests__")]);
    assert!(map.config_files.contains(&RepoPath::new("tsconfig.json")));
}

#[test]
fn maps_python_app_tests_migrations_and_ci() {
    let repo = TempRepo::new("architecture-map-python");
    repo.write(
        "pyproject.toml",
        "[project]\nname = \"architecture-map-python\"\n",
    );
    repo.write("app/routes/users.py", "def list_users():\n    return []\n");
    repo.write("app/services/users.py", "def users():\n    return []\n");
    repo.write("app/models/user.py", "class User:\n    pass\n");
    repo.write("migrations/001_init.py", "def upgrade():\n    pass\n");
    repo.write(
        "tests/test_users.py",
        "def test_users():\n    assert True\n",
    );
    repo.write(".github/workflows/ci.yml", "name: ci\n");

    let map = architecture_map(repo.root()).unwrap();

    assert_eq!(map.source_roots, vec![RepoPath::new("app")]);
    assert_eq!(map.test_roots, vec![RepoPath::new("tests")]);
    assert!(map
        .route_files
        .contains(&RepoPath::new("app/routes/users.py")));
    assert!(map
        .service_files
        .contains(&RepoPath::new("app/services/users.py")));
    assert!(map
        .model_files
        .contains(&RepoPath::new("app/models/user.py")));
    assert_eq!(
        map.migration_files,
        vec![RepoPath::new("migrations/001_init.py")]
    );
    assert_eq!(
        map.ci_files,
        vec![RepoPath::new(".github/workflows/ci.yml")]
    );
}
