//! Interface language selection (spec F-19): config `ui.lang` → system locale → English.

pub const LOCALE_EN: &str = "en";
pub const LOCALE_RU: &str = "ru";
pub const LOCALES: [&str; 2] = [LOCALE_EN, LOCALE_RU];

/// Picks the UI locale. A configured value must be one of `LOCALES` (case-insensitive); anything else is
/// ignored with a warning. The system locale counts when its language tag starts with a supported code.
pub fn resolve_locale(configured: Option<&str>, system: Option<&str>) -> &'static str {
	if let Some(configured) = configured {
		match LOCALES.iter().find(|locale| locale.eq_ignore_ascii_case(configured.trim())) {
			Some(locale) => return locale,
			None => tracing::warn!(configured, "unsupported ui.lang, falling back to the system locale"),
		}
	}
	match system {
		Some(tag) if tag.to_ascii_lowercase().starts_with(LOCALE_RU) => LOCALE_RU,
		_ => LOCALE_EN,
	}
}

pub fn system_locale() -> Option<String> {
	sys_locale::get_locale()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn configured_value_wins() {
		assert_eq!(resolve_locale(Some("ru"), Some("en-US")), "ru");
		assert_eq!(resolve_locale(Some(" EN "), Some("ru-RU")), "en");
	}

	#[test]
	fn unsupported_configured_value_falls_back_to_system() {
		assert_eq!(resolve_locale(Some("de"), Some("ru-RU")), "ru");
		assert_eq!(resolve_locale(Some("de"), Some("fr-FR")), "en");
	}

	#[test]
	fn system_locale_decides_when_nothing_configured() {
		assert_eq!(resolve_locale(None, Some("ru")), "ru");
		assert_eq!(resolve_locale(None, Some("RU-ru")), "ru");
		assert_eq!(resolve_locale(None, Some("en-GB")), "en");
		assert_eq!(resolve_locale(None, None), "en");
	}
}
