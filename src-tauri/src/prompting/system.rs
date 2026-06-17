use crate::domain::{GameProfile, ProjectConfig};
use crate::translation_io::TranslationResources;

pub fn build_system_prompt(config: &ProjectConfig, resources: &TranslationResources) -> String {
    let mut prompt = String::new();
    if !config.system_prompt.trim().is_empty() {
        prompt.push_str(config.system_prompt.trim());
        prompt.push_str("\n\n");
    }

    prompt.push_str("You translate RPG Maker MV/MZ game text.\n");
    prompt.push_str(&format!(
        "Translate from {} to {}.\n",
        config.source_lang, config.target_lang
    ));
    prompt.push_str("Return only JSON. Preserve line counts for multi-line source_text. Preserve every protected token exactly, including RPG Maker control codes and placeholders.\n");
    prompt.push_str("The user payload uses compact keys: f=shared file, sp=shared speaker, k=shared or item semantic kind, scene=shared event/page anchor, record=shared database record anchor, pre=shared preceding lines, post=shared following lines, g=shared terminology hits as [source,target] pairs, i=ordered items, src=source text, ctx=context, tok=protected tokens, n=notes, p=json path, r=record anchor, sc=scene anchor.\n");
    prompt.push_str("Items are ordered as they appear in the source. Read shared pre/post context, shared scene/record anchors, shared terminology hits, and earlier items to keep tone and terminology consistent across the whole batch.\n");
    prompt.push_str("When g supplies a source/target pair and the source term appears in the item or its local context, prefer that target wording consistently unless the user glossary explicitly overrides it.\n");
    prompt.push_str("Database files such as Armors.json, Items.json, Skills.json, Weapons.json, States.json, Actors.json, Classes.json, Enemies.json, Troops.json, and System.json contain RPG database records. Items whose json path shares the same array index, such as $[1].name, $[1].description, and $[1].note, describe the same record.\n");
    prompt.push_str("For k=name, use short game-like names or titles. For k=description, use natural flavorful prose. For k=text or note-like fields, preserve technical structure and translate only human-readable text.\n");
    prompt.push_str("Preserve script markers and labels such as Hint:, HintOffVN:, Reric:, Relic:, <...>, variable/control codes, numbers, and punctuation structure unless the user glossary explicitly says otherwise.\n");
    push_profile_prompt(&mut prompt, config.game_profile);
    prompt.push_str("Do not explain. Do not add markdown fences. Output shape: {\"translations\":[{\"id\":\"...\",\"translatedText\":\"...\"}]}.\n");

    if let Some(glossary) = resources.glossary.as_deref() {
        prompt.push_str("\nGlossary:\n");
        prompt.push_str(glossary.trim());
        prompt.push('\n');
    }

    if let Some(do_not_translate) = resources.do_not_translate.as_deref() {
        prompt.push_str("\nDo not translate these terms; copy them exactly:\n");
        prompt.push_str(do_not_translate.trim());
        prompt.push('\n');
    }

    prompt
}

fn push_profile_prompt(prompt: &mut String, profile: GameProfile) {
    match profile {
        GameProfile::GeneralRpg => {
            prompt.push_str("Game profile: general RPG. Keep translations natural, clear, and faithful. Do not add adult implications that are not present in the source or context.\n");
        }
        GameProfile::AdultRpg => {
            prompt.push_str("Game profile: adult RPG / H-game. Translate sexual or fetish content directly and naturally; do not sanitize, euphemize, or make explicit source text overly literary. Do not force sexual meanings onto ordinary game terms when context points elsewhere.\n");
            prompt.push_str("Adult RPG style defaults, unless the user glossary overrides them: 姫 can be 姬 in title-like names; 裸の姫 should prefer a natural title such as 裸姬, 露出姬, or 全裸公主 over stiff wording like 赤裸的公主; 開封中毒 with pack/card-pack context means 开包成瘾, not 开苞成瘾; Reric is a script label and should stay Reric unless the glossary says to change it.\n");
        }
        GameProfile::Custom => {
            prompt.push_str("Game profile: custom. Follow the user system prompt and glossary for tone, genre, and terminology. Still preserve RPG Maker control codes and technical note structure.\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ApiEndpoint, BatchingStrategy, GameProfile};

    #[test]
    fn adult_profile_keeps_adult_content_without_oversexualizing_context() {
        let prompt = build_system_prompt(
            &config(GameProfile::AdultRpg),
            &TranslationResources::default(),
        );

        assert!(prompt.contains("do not sanitize"));
        assert!(prompt.contains("Do not force sexual meanings"));
        assert!(prompt.contains("開封中毒"));
        assert!(prompt.contains("开包成瘾"));
        assert!(prompt.contains("not 开苞成瘾"));
        assert!(prompt.contains("裸の姫"));
        assert!(prompt.contains("赤裸的公主"));
    }

    #[test]
    fn prompt_explains_database_record_context_and_note_markers() {
        let prompt = build_system_prompt(
            &config(GameProfile::GeneralRpg),
            &TranslationResources::default(),
        );

        assert!(prompt.contains("Armors.json"));
        assert!(prompt.contains("$[1].name"));
        assert!(prompt.contains("$[1].description"));
        assert!(prompt.contains("$[1].note"));
        assert!(prompt.contains("g=shared terminology hits"));
        assert!(prompt.contains("Hint:"));
        assert!(prompt.contains("HintOffVN:"));
        assert!(prompt.contains("Reric:"));
        assert!(prompt.contains("variable/control codes"));
    }

    fn config(game_profile: GameProfile) -> ProjectConfig {
        ProjectConfig {
            game_root: String::new(),
            model: "gpt-4.1-mini".to_owned(),
            api_endpoint: ApiEndpoint::Responses,
            api_key: None,
            base_url: None,
            system_prompt: String::new(),
            glossary_path: None,
            do_not_translate_path: None,
            game_profile,
            target_input_tokens: crate::domain::default_target_input_tokens(),
            batching_strategy: BatchingStrategy::MaximizeUtilization,
            debug_logging: false,
            max_concurrency: 1,
            request_timeout_secs: 90,
            source_lang: "Japanese".to_owned(),
            target_lang: "Chinese".to_owned(),
        }
    }
}
