// SPDX-License-Identifier: GPL-3.0-or-later
//! Compiles `ui/shell.slint` and bundles its translations (`i18n/`).

fn main() {
    // The default translation context wraps every `@tr()` in a check that prevents translations
    // from applying unless explicitly turned off (spike 2's own pitfall, architecture §10.2).
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("i18n")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/shell.slint", config).unwrap();
}
