//! Settings persistence for Syn (Local AI Chat).
//!
//! Loads and saves user-configurable settings from `{vault}/Syn/settings.json`.
//! Falls back to sensible defaults if the file doesn't exist or is corrupted.

use crate::error::{AppError, AppResult};
use crate::models::syn::SynSettings;
use std::path::Path;

/// Load settings from `{vault}/Syn/settings.json`.
/// Returns defaults if the file doesn't exist or contains invalid JSON.
pub fn load_settings(vault_path: &str) -> AppResult<SynSettings> {
    let settings_path = Path::new(vault_path).join("Syn").join("settings.json");
    if !settings_path.exists() {
        return Ok(SynSettings::default());
    }
    let content = std::fs::read_to_string(&settings_path)?;
    let settings: SynSettings = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "[Syn] Settings file contains invalid JSON, using defaults: {}",
                e
            );
            SynSettings::default()
        }
    };
    Ok(settings)
}

/// Save settings to `{vault}/Syn/settings.json`.
/// Creates the `Syn/` directory if it doesn't exist.
pub fn save_settings(vault_path: &str, settings: &SynSettings) -> AppResult<()> {
    let syn_dir = Path::new(vault_path).join("Syn");
    std::fs::create_dir_all(&syn_dir)
        .map_err(|e| AppError::General(format!("Failed to create Syn directory: {}", e)))?;
    let settings_path = syn_dir.join("settings.json");
    let json = serde_json::to_string_pretty(settings)?;
    let tmp_path = settings_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &settings_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        AppError::General(format!("Failed to rename temp settings file: {}", e))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::syn::SynProvider;

    /// A vault written before the switch existed is a vault where Syn was on.
    ///
    /// The alternative — `#[serde(default)]` on a `bool`, which is `false` —
    /// would have switched Syn off for every existing user on upgrade, and the
    /// only clue would have been the app going quiet.
    #[test]
    fn a_settings_file_that_predates_the_switch_still_has_syn_on() {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = dir.path().to_str().expect("utf8");
        std::fs::create_dir_all(dir.path().join("Syn")).expect("Syn dir");
        std::fs::write(
            dir.path().join("Syn").join("settings.json"),
            r#"{"ollama_url":"http://localhost:11434","temperature":0.7,"rag_enabled":true,
                "max_context_chars":12000,"include_finance":true,"include_feeds":true,
                "graph_expansion_depth":1,"personality":"auto","custom_system_prompt":null,
                "default_model":null}"#,
        )
        .expect("written");

        assert!(load_settings(vault).expect("loads").enabled);
    }

    /// And switching it off survives a round trip, which is the whole point of
    /// it being a setting rather than a runtime flag.
    #[test]
    fn switching_syn_off_is_remembered() {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = dir.path().to_str().expect("utf8");

        let mut settings = SynSettings::default();
        settings.enabled = false;
        save_settings(vault, &settings).expect("saved");

        assert!(!load_settings(vault).expect("loads").enabled);
    }

    /// The frontend tells "Syn is off" apart from "the provider is unreachable"
    /// by matching on this string, and the two mean opposite things about
    /// whether anything is wrong. Read out of the TypeScript rather than
    /// duplicated, so a reworded refusal fails here instead of silently showing
    /// somebody an error for a choice they made.
    #[test]
    fn the_frontend_recognises_the_refusal_by_its_words() {
        let source = include_str!("../../../src/mini-apps/messages/composables/useSynChat.ts");
        assert!(
            source.contains(crate::commands::syn::SWITCHED_OFF),
            "`{}` no longer appears in useSynChat.ts",
            crate::commands::syn::SWITCHED_OFF
        );
    }

    /// The strings the frontend sends back.
    ///
    /// `useSynSettings.ts` types `provider` as `'ollama' | 'open_ai_compat'`
    /// and writes those literals into the `<select>`. A rename on this side
    /// would not fail to compile anywhere — it would produce a settings file
    /// whose `provider` no longer deserialises, fall back to the default, and
    /// send every message to Ollama while the UI showed OpenAI.
    #[test]
    fn the_provider_names_are_the_ones_the_frontend_writes() {
        assert_eq!(
            serde_json::to_value(SynProvider::Ollama).expect("serialises"),
            serde_json::json!("ollama")
        );
        assert_eq!(
            serde_json::to_value(SynProvider::OpenAiCompat).expect("serialises"),
            serde_json::json!("open_ai_compat")
        );
        assert_eq!(
            serde_json::to_value(SynProvider::Gemini).expect("serialises"),
            serde_json::json!("gemini")
        );

        // And the other side actually writes them. Checking the Rust names
        // alone would pass while the TypeScript union still lacked one — and a
        // provider missing there is a `<select>` whose value Vue cannot bind.
        let ts = include_str!("../../../src/mini-apps/messages/composables/useSynSettings.ts");
        for provider in [SynProvider::Ollama, SynProvider::OpenAiCompat, SynProvider::Gemini] {
            let name = serde_json::to_value(provider).expect("serialises");
            let quoted = format!("'{}'", name.as_str().expect("a string"));
            assert!(ts.contains(&quoted), "useSynSettings.ts does not know {quoted}");
        }
    }

    /// Each provider's default survives the round trip through the file, under
    /// the name the frontend writes for it — and a file written before the map
    /// existed still loads, with nothing in it.
    #[test]
    fn each_provider_keeps_its_own_default_model() {
        let mut s = SynSettings::default();
        s.default_models.insert(SynProvider::OpenAiCompat, "gpt-5.6-luna".into());
        s.default_models.insert(SynProvider::Gemini, "gemini-3.8-flash".into());

        let json = serde_json::to_value(&s).expect("serialises");
        assert_eq!(json["default_models"]["open_ai_compat"], "gpt-5.6-luna");
        assert_eq!(json["default_models"]["gemini"], "gemini-3.8-flash");

        let back: SynSettings = serde_json::from_value(json).expect("reads back");
        assert_eq!(back.default_models.get(&SynProvider::Gemini).map(String::as_str), Some("gemini-3.8-flash"));

        let old: SynSettings = serde_json::from_value(serde_json::json!({
            "provider": "open_ai_compat",
            "default_model": "gpt-5.6-luna",
            "temperature": 0.7, "max_tool_iterations": 12, "rag_enabled": true,
            "max_context_chars": 12000, "include_finance": true, "include_feeds": true,
            "graph_expansion_depth": 1, "num_ctx": 8192, "max_history_messages": 50,
            "ollama_url": "http://localhost:11434"
        }))
        .expect("a file from before the map still loads");
        assert!(old.default_models.is_empty());
        assert_eq!(old.default_model.as_deref(), Some("gpt-5.6-luna"));
    }

    /// Each provider that takes a key has a slot of its own.
    ///
    /// Two providers sharing one would mean saving a Gemini key overwrote an
    /// OpenAI one, silently, and the first sign would be a 401 on the provider
    /// the person had not touched.
    #[test]
    fn every_provider_files_its_key_somewhere_different() {
        let slots = [
            SynProvider::Ollama.key_slot(),
            SynProvider::OpenAiCompat.key_slot(),
            SynProvider::Gemini.key_slot(),
        ];
        let unique: std::collections::HashSet<_> = slots.iter().collect();
        assert_eq!(unique.len(), slots.len(), "{slots:?}");
    }

    /// Only a model on this machine is local; everything hosted is sized as
    /// hosted. This is what a web page's length turns on.
    #[test]
    fn only_ollama_is_local() {
        assert!(SynProvider::Ollama.is_local());
        assert!(!SynProvider::OpenAiCompat.is_local());
        assert!(!SynProvider::Gemini.is_local());
    }

    /// The two default tables have to agree.
    ///
    /// `SynSettings::default()` fills a vault that has no settings file;
    /// `DEFAULT_SETTINGS` in `useSynSettings.ts` is what the Reset button
    /// restores and what the screen falls back to when the file cannot be
    /// read. Nothing links them, and they had already drifted: the frontend
    /// held `num_ctx: 4096`, `max_context_chars: 32000` and
    /// `max_history_messages: 20` against the backend's 8192, 12000 and 50, so
    /// pressing Reset produced a configuration no fresh vault has ever had.
    ///
    /// Read out of the source rather than duplicated here, so this cannot
    /// drift in the same way the thing it guards did.
    #[test]
    fn the_frontend_defaults_match_the_ones_a_fresh_vault_gets() {
        let source = include_str!("../../../src/mini-apps/messages/composables/useSynSettings.ts");
        let block = source
            .split("const DEFAULT_SETTINGS: SynSettings = {")
            .nth(1)
            .expect("DEFAULT_SETTINGS is declared")
            .split("};")
            .next()
            .expect("the declaration closes");

        let declared = |key: &str| -> String {
            block
                .lines()
                .find_map(|l| l.trim().strip_prefix(&format!("{key}:")))
                .unwrap_or_else(|| panic!("`{key}` is missing from DEFAULT_SETTINGS"))
                .trim()
                .trim_end_matches(',')
                .trim_matches('\'')
                .to_string()
        };

        let d = SynSettings::default();
        // The switch most of all: a frontend that reset `enabled` to false
        // would leave somebody pressing Reset and finding Syn gone, with the
        // one control that brings it back on the screen they just reset.
        assert_eq!(declared("enabled"), d.enabled.to_string());
        assert!(d.enabled, "a fresh vault has Syn on");
        assert_eq!(declared("num_ctx"), d.num_ctx.to_string());
        assert_eq!(declared("max_history_messages"), d.max_history_messages.to_string());
        assert_eq!(declared("max_context_chars"), d.max_context_chars.to_string());
        // `null`, and it has to be: it means "let the provider decide", which
        // is a different answer from any number and the reason a laptop and a
        // hosted model are not given the same slice of a page.
        assert_eq!(declared("max_page_chars"), "null");
        assert!(d.max_page_chars.is_none());
        assert_eq!(declared("max_tool_iterations"), d.max_tool_iterations.to_string());
        assert_eq!(declared("temperature"), d.temperature.to_string());
        assert_eq!(declared("memory_reflection"), d.memory_reflection.to_string());
        assert_eq!(declared("ollama_url"), d.ollama_url);
        assert_eq!(declared("openai_base_url"), d.openai_base_url);
        assert_eq!(
            declared("provider"),
            serde_json::to_value(d.provider)
                .expect("serialises")
                .as_str()
                .expect("a string")
        );
    }

    /// A settings file written before the ceiling was raised keeps its own
    /// value; only a vault that never said gets the new default.
    #[test]
    fn an_existing_ceiling_is_not_overwritten_by_the_new_default() {
        let pinned: SynSettings =
            serde_json::from_str(r#"{"ollama_url":"http://localhost:11434","default_model":null,
                "temperature":0.7,"max_tool_iterations":5,"rag_enabled":true,
                "max_context_chars":12000,"include_finance":true,"include_feeds":true,
                "graph_expansion_depth":1,"personality":"auto","custom_system_prompt":null}"#)
                .expect("loads");
        assert_eq!(pinned.max_tool_iterations, 5);

        let absent: SynSettings =
            serde_json::from_str(r#"{"ollama_url":"http://localhost:11434","default_model":null,
                "temperature":0.7,"rag_enabled":true,
                "max_context_chars":12000,"include_finance":true,"include_feeds":true,
                "graph_expansion_depth":1,"personality":"auto","custom_system_prompt":null}"#)
                .expect("loads");
        assert_eq!(absent.max_tool_iterations, 12);
    }

    /// A vault written before providers existed has no `provider` key, and it
    /// is an Ollama vault. Anything else would move a working setup onto an
    /// endpoint it has no key for.
    #[test]
    fn settings_without_a_provider_are_ollama_settings() {
        let old = r#"{
            "ollama_url": "http://localhost:11434",
            "default_model": "llama3.2",
            "temperature": 0.7,
            "max_tool_iterations": 5,
            "rag_enabled": true,
            "max_context_chars": 12000,
            "include_finance": true,
            "include_feeds": true,
            "graph_expansion_depth": 1,
            "personality": "auto",
            "custom_system_prompt": null
        }"#;

        let parsed: SynSettings = serde_json::from_str(old).expect("an old settings file loads");
        assert_eq!(parsed.provider, SynProvider::Ollama);
        assert_eq!(parsed.openai_base_url, "https://api.openai.com/v1");
        assert_eq!(parsed.default_model.as_deref(), Some("llama3.2"));
    }

    #[test]
    fn settings_survive_a_round_trip_through_the_vault() {
        let dir = tempfile::tempdir().expect("temp dir");
        let vault = dir.path().to_str().expect("utf8 path");

        // Nothing written yet: defaults, not an error.
        assert_eq!(
            load_settings(vault).expect("defaults").provider,
            SynProvider::Ollama
        );

        let mut settings = SynSettings::default();
        settings.provider = SynProvider::OpenAiCompat;
        settings.openai_base_url = "http://localhost:8080/v1".to_string();
        save_settings(vault, &settings).expect("saves");

        let read_back = load_settings(vault).expect("loads");
        assert_eq!(read_back.provider, SynProvider::OpenAiCompat);
        assert_eq!(read_back.openai_base_url, "http://localhost:8080/v1");
    }

    /// The vault is the wrong place for a credential, and this is the test
    /// that says so out loud: `settings.json` syncs between devices and is
    /// committed on a vault kept in git.
    #[test]
    fn no_secret_is_ever_written_into_the_vault() {
        let json = serde_json::to_string(&SynSettings::default()).expect("serialises");
        let lowered = json.to_lowercase();
        for forbidden in ["api_key", "apikey", "token", "secret", "password"] {
            assert!(
                !lowered.contains(forbidden),
                "SynSettings is written into the vault and must not carry `{forbidden}`: {json}"
            );
        }
    }
}
