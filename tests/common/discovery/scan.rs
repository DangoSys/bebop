use std::ffi::OsStr;
use std::path::Path;
use walkdir::WalkDir;

use super::test_case::ElfTestCase;

pub fn scan_elf_files(root: &Path, extension: Option<&str>) -> Vec<ElfTestCase> {
    let mut tests = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if let Some(ext) = extension {
            if path.extension() != Some(OsStr::new(ext)) {
                continue;
            }
        }

        if let Some(test_case) = ElfTestCase::from_path(path.to_path_buf()) {
            tests.push(test_case);
        }
    }

    tests.sort_by(|a, b| a.name.cmp(&b.name));
    tests
}

/// Walk `root` and collect test cases whose stem is in `stems`.
/// Stems are matched without file extension; extension filtering is applied
/// when `extension` is `Some(_)`.
///
/// Returns the matched test cases (in the same order as `stems`, with
/// duplicates collapsed) and the list of stems that could not be found.
pub fn scan_elf_files_by_stems(
    root: &Path,
    extension: Option<&str>,
    stems: &[String],
) -> (Vec<ElfTestCase>, Vec<String>) {
    use std::collections::HashMap;

    let mut found: HashMap<String, ElfTestCase> = HashMap::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if let Some(ext) = extension {
            if path.extension() != Some(OsStr::new(ext)) {
                continue;
            }
        }

        let Some(test_case) = ElfTestCase::from_path(path.to_path_buf()) else {
            continue;
        };

        if stems.iter().any(|s| s == &test_case.stem) && !found.contains_key(&test_case.stem) {
            found.insert(test_case.stem.clone(), test_case);
        }
    }

    let mut selected = Vec::new();
    let mut missing = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for stem in stems {
        if !seen.insert(stem.clone()) {
            continue;
        }
        match found.remove(stem) {
            Some(tc) => selected.push(tc),
            None => missing.push(stem.clone()),
        }
    }
    (selected, missing)
}

pub fn filter_tests(
    tests: Vec<ElfTestCase>,
    filter: Option<&str>,
    match_case: impl Fn(&ElfTestCase) -> bool,
) -> Vec<ElfTestCase> {
    let tests: Vec<_> = match filter {
        Some(pattern) => tests
            .into_iter()
            .filter(|t| t.name.contains(pattern) || t.stem.contains(pattern))
            .collect(),
        None => tests,
    };

    tests.into_iter().filter(match_case).collect()
}
