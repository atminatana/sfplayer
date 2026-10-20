//! README hotkey tables must come from the same binding table as the F1 help (ADR 0004).
//! `README.md` is English, `README.ru.md` is Russian; each carries the table in its own language.

use sf2_player::ui::keys::Bindings;

const START: &str = "<!-- keys:start -->";
const END: &str = "<!-- keys:end -->";

/// README file and the locale of its hotkey table.
const READMES: [(&str, &str); 2] = [("README.md", "en"), ("README.ru.md", "ru")];

/// Markdown table for the default bindings, descriptions in the README's language.
fn keys_table(locale: &str) -> String {
	let header = match locale {
		"ru" => "| Клавиши | Действие |\n|---|---|\n",
		_ => "| Keys | Action |\n|---|---|\n",
	};
	let mut table = String::from(header);
	for (keys, description) in Bindings::default().help_rows(locale) {
		table.push_str(&format!("| {} | {} |\n", keys.replace('|', r"\|"), description));
	}
	table
}

/// `cargo test --test readme_keys print_table -- --nocapture` prints the blocks to paste into both READMEs.
#[test]
fn print_table() {
	for (file, locale) in READMES {
		println!("=== {file} ===");
		print!("{}", keys_table(locale));
	}
}

#[test]
fn readmes_contain_the_generated_hotkey_tables() {
	for (file, locale) in READMES {
		let path = format!("{}/{file}", env!("CARGO_MANIFEST_DIR"));
		let readme = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{file}: {e}"));
		let start = readme.find(START).unwrap_or_else(|| panic!("{file}: keys:start marker"));
		let end = readme.find(END).unwrap_or_else(|| panic!("{file}: keys:end marker"));
		let block = readme[start + START.len()..end].trim();
		assert_eq!(
			block,
			keys_table(locale).trim(),
			"{file} hotkey table is stale; regenerate it with the print_table test"
		);
	}
}
