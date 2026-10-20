//! README hotkey table must come from the same binding table as the F1 help (ADR 0004).

use sf2_player::ui::keys::Bindings;

const START: &str = "<!-- keys:start -->";
const END: &str = "<!-- keys:end -->";

/// Markdown table for the default bindings, Russian descriptions (README language).
fn keys_table() -> String {
	let mut table = String::from("| Клавиши | Действие |\n|---|---|\n");
	for (keys, description) in Bindings::default().help_rows("ru") {
		table.push_str(&format!("| {} | {} |\n", keys.replace('|', r"\|"), description));
	}
	table
}

/// `cargo test --test readme_keys print_table -- --nocapture` prints the block to paste into README.md.
#[test]
fn print_table() {
	print!("{}", keys_table());
}

#[test]
fn readme_contains_the_generated_hotkey_table() {
	let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).expect("README.md");
	let start = readme.find(START).expect("keys:start marker");
	let end = readme.find(END).expect("keys:end marker");
	let block = readme[start + START.len()..end].trim();
	assert_eq!(
		block,
		keys_table().trim(),
		"README hotkey table is stale; regenerate it with the print_table test"
	);
}
