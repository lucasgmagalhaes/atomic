use shell::i18n::{self, Locale};

const ALL_KEYS: &[&str] = &[
    i18n::PANES_LABEL,
    i18n::SELECTED_LABEL,
    i18n::RELOAD_BUTTON,
    i18n::ADDRESS_BAR_HINT,
    i18n::PROXY_LABEL,
    i18n::PROXY_HINT,
    i18n::APPLY_BUTTON,
    i18n::FAILED_TO_LOAD_PREFIX,
    i18n::AUTOMATION_HEADER,
    i18n::AUTOMATION_SCRIPT_HINT,
    i18n::RUN_BUTTON,
    i18n::STARTING_PROFILE,
];

#[test]
fn every_key_has_a_non_empty_translation_in_both_locales() {
    for &key in ALL_KEYS {
        assert!(!i18n::t(key, Locale::En).is_empty(), "missing EN for {key}");
        assert!(!i18n::t(key, Locale::Pt).is_empty(), "missing PT for {key}");
    }
}

#[test]
fn en_and_pt_differ_for_translated_labels() {
    // The automation script hint is deliberately identical (example code,
    // not prose) - every other key should actually be translated.
    for &key in ALL_KEYS {
        if key == i18n::AUTOMATION_SCRIPT_HINT {
            continue;
        }
        assert_ne!(i18n::t(key, Locale::En), i18n::t(key, Locale::Pt), "{key} is identical in both locales");
    }
}

#[test]
fn known_keys_return_the_expected_english_text() {
    assert_eq!(i18n::t(i18n::RELOAD_BUTTON, Locale::En), "Reload");
    assert_eq!(i18n::t(i18n::APPLY_BUTTON, Locale::En), "Apply");
    assert_eq!(i18n::t(i18n::RUN_BUTTON, Locale::En), "Run");
}

#[test]
fn known_keys_return_the_expected_portuguese_text() {
    assert_eq!(i18n::t(i18n::RELOAD_BUTTON, Locale::Pt), "Recarregar");
    assert_eq!(i18n::t(i18n::APPLY_BUTTON, Locale::Pt), "Aplicar");
    assert_eq!(i18n::t(i18n::RUN_BUTTON, Locale::Pt), "Executar");
}

#[test]
fn missing_key_falls_back_to_the_key_itself_in_either_locale() {
    let missing = "this_key_does_not_exist";
    assert_eq!(i18n::t(missing, Locale::En), missing);
    assert_eq!(i18n::t(missing, Locale::Pt), missing);
}
