//! EN/PT string table for `apps/shell`'s UI (closes the "Interface i18n"
//! spec gap). `main.rs`'s toolbar/panel calls [`t`] (and [`fill`] for the
//! two templates with `{}` placeholders) for every string listed below,
//! selected by `NimbleApp.locale` and toggled live via EN/PT buttons in
//! the toolbar - no restart needed, since every string is looked up fresh
//! each frame. Still hardcoded in `main.rs`: the per-pane context menu's
//! labels (Reload, Duplicate profile, ...) and the disabled Mute audio/Dev
//! tools items - not migrated in this pass.
//!
//! No external i18n crate: the string set is small and fixed, so a plain
//! match is both the simplest and the most idiomatic fit.

/// Supported UI locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    Pt,
}

/// Keys for every UI string `main.rs` currently hardcodes. Format-style
/// strings (e.g. "Selected: {}") keep their `{}`/`{error}` placeholders
/// literally in the table - the caller still does its own `format!`, this
/// module only owns the translated template text.
pub const PANES_LABEL: &str = "panes_label";
pub const SELECTED_LABEL: &str = "selected_label";
pub const RELOAD_BUTTON: &str = "reload_button";
pub const ADDRESS_BAR_HINT: &str = "address_bar_hint";
pub const PROXY_LABEL: &str = "proxy_label";
pub const PROXY_HINT: &str = "proxy_hint";
pub const APPLY_BUTTON: &str = "apply_button";
pub const FAILED_TO_LOAD_PREFIX: &str = "failed_to_load_prefix";
pub const AUTOMATION_HEADER: &str = "automation_header";
pub const AUTOMATION_SCRIPT_HINT: &str = "automation_script_hint";
pub const RUN_BUTTON: &str = "run_button";
pub const STARTING_PROFILE: &str = "starting_profile";

/// Falls back to the key itself when a locale is missing a translation -
/// keeps the UI showing *something* recognizable instead of panicking or
/// rendering an empty label.
pub fn t(key: &str, locale: Locale) -> &str {
    match (key, locale) {
        (PANES_LABEL, Locale::En) => "Panes:",
        (PANES_LABEL, Locale::Pt) => "Painéis:",

        (SELECTED_LABEL, Locale::En) => "Selected: {}",
        (SELECTED_LABEL, Locale::Pt) => "Selecionado: {}",

        (RELOAD_BUTTON, Locale::En) => "Reload",
        (RELOAD_BUTTON, Locale::Pt) => "Recarregar",

        (ADDRESS_BAR_HINT, Locale::En) => "Enter a URL for the selected pane and press Enter",
        (ADDRESS_BAR_HINT, Locale::Pt) => "Digite uma URL para o painel selecionado e pressione Enter",

        (PROXY_LABEL, Locale::En) => "Proxy (selected pane):",
        (PROXY_LABEL, Locale::Pt) => "Proxy (painel selecionado):",

        (PROXY_HINT, Locale::En) => "host:port (empty = none)",
        (PROXY_HINT, Locale::Pt) => "host:porta (vazio = nenhum)",

        (APPLY_BUTTON, Locale::En) => "Apply",
        (APPLY_BUTTON, Locale::Pt) => "Aplicar",

        (FAILED_TO_LOAD_PREFIX, Locale::En) => "Failed to load",
        (FAILED_TO_LOAD_PREFIX, Locale::Pt) => "Falha ao carregar",

        (AUTOMATION_HEADER, Locale::En) => "Automation — pane names from the active workspace (\"{}\"): {}",
        (AUTOMATION_HEADER, Locale::Pt) => "Automação — nomes dos painéis do workspace ativo (\"{}\"): {}",

        (AUTOMATION_SCRIPT_HINT, Locale::En) => r#"pane("pane-1").goto("https://example.com")"#,
        (AUTOMATION_SCRIPT_HINT, Locale::Pt) => r#"pane("pane-1").goto("https://example.com")"#,

        (RUN_BUTTON, Locale::En) => "Run",
        (RUN_BUTTON, Locale::Pt) => "Executar",

        (STARTING_PROFILE, Locale::En) => "Starting profile process...",
        (STARTING_PROFILE, Locale::Pt) => "Iniciando processo do perfil...",

        (unknown, _) => unknown,
    }
}

/// Fills a `t`-style template's positional `{}` placeholders, left to
/// right - a stand-in for `format!` (which needs a string literal, not a
/// runtime `&str` from this table) that only needs to handle the small,
/// fixed set of one/two-placeholder templates this table actually has.
pub fn fill(template: &str, args: &[&str]) -> String {
    let mut out = template.to_string();
    for arg in args {
        out = out.replacen("{}", arg, 1);
    }
    out
}
