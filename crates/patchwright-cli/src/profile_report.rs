use crate::render_utils::push_plain_list;
use patchwright_core::types::RepoPath;
use patchwright_index::ProjectProfile;

pub(crate) fn render_project_profile(profile: &ProjectProfile) -> String {
    let mut output = String::new();

    output.push_str("Patchwright project profile\n\n");
    push_plain_list(
        &mut output,
        "Detected languages",
        &profile.detected_languages,
    );
    let manifests = repo_paths_as_strings(&profile.manifests);
    push_plain_list(&mut output, "Manifests", &manifests);
    push_plain_list(&mut output, "Package managers", &profile.package_managers);
    push_plain_list(
        &mut output,
        "Likely install commands",
        &profile.install_commands,
    );
    push_plain_list(
        &mut output,
        "Likely build commands",
        &profile.build_commands,
    );
    push_plain_list(&mut output, "Likely test commands", &profile.test_commands);
    push_plain_list(
        &mut output,
        "Likely typecheck commands",
        &profile.typecheck_commands,
    );
    push_plain_list(&mut output, "Likely lint commands", &profile.lint_commands);
    let source_roots = repo_paths_as_strings(&profile.source_roots);
    push_plain_list(&mut output, "Source roots", &source_roots);
    let test_roots = repo_paths_as_strings(&profile.test_roots);
    push_plain_list(&mut output, "Test roots", &test_roots);
    let ci_files = repo_paths_as_strings(&profile.ci_files);
    push_plain_list(&mut output, "CI files", &ci_files);

    output
}

fn repo_paths_as_strings(paths: &[RepoPath]) -> Vec<String> {
    paths.iter().map(|path| path.0.clone()).collect()
}
