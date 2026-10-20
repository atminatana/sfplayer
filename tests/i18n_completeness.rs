//! Spec F-19 (ADR 0008): every UI string lives in the dictionary, in both languages.
//!
//! Checks the repository files directly: `locales/app.yml` (rust-i18n v2 layout) and the sources under `src/`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const LOCALES: [&str; 2] = ["en", "ru"];
/// Dictionary namespaces; a string literal starting with one of these followed by a dot is treated as a key.
const NAMESPACES: [&str; 11] = [
	"app", "header", "panel", "status", "hint", "notice", "dialog", "help", "mixer", "meter", "confirm",
];

fn root() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Parses the v2 file: top-level `key:` lines followed by indented `locale: "text"` lines.
fn dictionary() -> BTreeMap<String, BTreeMap<String, String>> {
	let text = fs::read_to_string(root().join("locales/app.yml")).expect("locales/app.yml");
	let mut keys: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
	let mut current: Option<String> = None;
	for (number, line) in text.lines().enumerate() {
		if line.trim().is_empty() || line.trim_start().starts_with('#') {
			continue;
		}
		if !line.starts_with(' ') {
			if line.starts_with('_') {
				// `_version: 2` and other file-level settings.
				current = None;
				continue;
			}
			let key = line
				.strip_suffix(':')
				.unwrap_or_else(|| panic!("line {}: top-level line without ':': {line}", number + 1));
			current = Some(key.to_owned());
			assert!(keys.insert(key.to_owned(), BTreeMap::new()).is_none(), "duplicate key {key}");
		} else {
			let key = current
				.clone()
				.unwrap_or_else(|| panic!("line {}: translation before any key", number + 1));
			let (locale, value) = line
				.trim()
				.split_once(':')
				.unwrap_or_else(|| panic!("line {}: expected 'locale: text'", number + 1));
			let value = value.trim().trim_matches('"').to_owned();
			keys.get_mut(&key).unwrap().insert(locale.trim().to_owned(), value);
		}
	}
	keys
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
	for entry in fs::read_dir(dir).expect("readable dir") {
		let path = entry.expect("entry").path();
		if path.is_dir() {
			rust_sources(&path, out);
		} else if path.extension().is_some_and(|extension| extension == "rs") {
			out.push(path);
		}
	}
}

/// Source text without comments, without inline `#[cfg(test)]` modules and without `*/tests.rs` files.
fn production_sources() -> Vec<(PathBuf, String)> {
	let mut files = Vec::new();
	rust_sources(&root().join("src"), &mut files);
	files
		.into_iter()
		.filter(|path| path.file_name().is_some_and(|name| name != "tests.rs"))
		.map(|path| {
			let text = fs::read_to_string(&path).expect("source");
			let production = match text.find("#[cfg(test)]") {
				Some(index) => text[..index].to_owned(),
				None => text,
			};
			// Comments are prose, not UI strings.
			let without_comments: String = production
				.lines()
				.filter(|line| !line.trim_start().starts_with("//"))
				.map(|line| line.split(" // ").next().unwrap_or(line))
				.collect::<Vec<_>>()
				.join(
					"
",
				);
			(path, without_comments)
		})
		.collect()
}

/// All `"..."` literals in a source text (no escape handling needed for the keys we look for).
fn string_literals(text: &str) -> Vec<String> {
	let mut literals = Vec::new();
	let mut rest = text;
	while let Some(start) = rest.find('"') {
		let after = &rest[start + 1..];
		let Some(end) = after.find('"') else { break };
		literals.push(after[..end].to_owned());
		rest = &after[end + 1..];
	}
	literals
}

#[test]
fn every_key_has_both_languages_and_nothing_else() {
	let keys = dictionary();
	assert!(keys.len() > 100, "dictionary looks truncated: {} keys", keys.len());
	for (key, translations) in &keys {
		let locales: BTreeSet<&str> = translations.keys().map(String::as_str).collect();
		let expected: BTreeSet<&str> = LOCALES.into_iter().collect();
		assert_eq!(locales, expected, "key {key} must have exactly en and ru");
		for (locale, value) in translations {
			assert!(!value.trim().is_empty(), "{key}.{locale} is empty");
		}
	}
}

#[test]
fn every_dictionary_key_used_in_code_exists() {
	let keys = dictionary();
	let mut missing = BTreeSet::new();
	let mut used = BTreeSet::new();
	for (path, text) in production_sources() {
		for literal in string_literals(&text) {
			let is_key = NAMESPACES.iter().any(|namespace| literal.starts_with(&format!("{namespace}.")))
				&& literal
					.chars()
					.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_');
			if is_key {
				used.insert(literal.clone());
				if !keys.contains_key(&literal) {
					missing.insert(format!("{literal} ({})", path.display()));
				}
			}
		}
	}
	assert!(
		missing.is_empty(),
		"keys used in code but absent from locales/app.yml:\n{}",
		missing.into_iter().collect::<Vec<_>>().join("\n")
	);
	assert!(
		used.len() > 50,
		"key scan found only {} keys; the scanner is probably broken",
		used.len()
	);
}

#[test]
fn no_cyrillic_literals_outside_the_dictionary() {
	let mut offenders = Vec::new();
	for (path, text) in production_sources() {
		for literal in string_literals(&text) {
			if literal.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)) {
				offenders.push(format!("{}: {literal:?}", path.display()));
			}
		}
	}
	assert!(
		offenders.is_empty(),
		"hard-coded Russian text in production code:\n{}",
		offenders.join("\n")
	);
}
