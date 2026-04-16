use std::io::{self, Write};

use anyhow::Result;

use crate::config::{
    is_real_value,
    providers::{DOUBAO_VOICES, LLM_PRESETS},
    AppConfig, LlmProfile, CONFIG_PATH_DISPLAY,
};
use crate::ui::{
    select::{self, SelectOption},
    theme::*,
};

// ─── Public entry points ──────────────────────────────────────────────────────

pub async fn run_wizard() -> Result<()> {
    print_wizard_header();
    let mut cfg = AppConfig::load().unwrap_or_default();

    wizard_persona(&mut cfg)?;
    wizard_llm_profiles(&mut cfg)?;
    wizard_speech(&mut cfg)?;
    wizard_voice(&mut cfg)?;

    cfg.save()?;
    println!();
    println!("   {BR_GREEN}✓{RESET}  已保存 → {MUTED}{CONFIG_PATH_DISPLAY}{RESET}");
    println!();
    Ok(())
}

/// Minimal first-run prompt for missing required fields.
pub fn ensure_config(mut cfg: AppConfig) -> Result<AppConfig> {
    if cfg.is_complete() {
        return Ok(cfg);
    }
    println!();
    println!(
        "   {BR_YELLOW}⚠  缺少配置{RESET}  {MUTED}(运行 `cb config` 进入完整向导){RESET}"
    );
    println!();

    if cfg.active_llm_profile().is_none() || !is_real_value(&cfg.active_llm_profile().unwrap().api_key) {
        // Quick setup: pick a preset or enter custom
        println!("   {BOLD}选择 LLM 供应商:{RESET}");
        let opts: Vec<SelectOption> = LLM_PRESETS
            .iter()
            .map(|p| SelectOption::new(p.name, p.base_url))
            .chain(std::iter::once(SelectOption::new("Custom...", "")))
            .collect();

        let choice = select::run(&opts, 0)?.unwrap_or(0);
        println!();

        let profile = if choice < LLM_PRESETS.len() {
            let preset = &LLM_PRESETS[choice];
            let api_key = if preset.needs_key {
                prompt_required(&format!("   {} API Key", preset.name))?
            } else {
                "ollama".to_string()
            };
            LlmProfile {
                name: preset.name.to_string(),
                base_url: preset.base_url.to_string(),
                model: preset.default_model.to_string(),
                api_key,
            }
        } else {
            let base_url = prompt_required("   Base URL")?;
            let model = prompt_required("   Model")?;
            let api_key = prompt_required("   API Key")?;
            LlmProfile {
                name: "Custom".to_string(),
                base_url,
                model,
                api_key,
            }
        };

        let name = profile.name.clone();
        if let Some(existing) = cfg.llm_profiles.iter_mut().find(|p| p.name == profile.name) {
            *existing = profile;
        } else {
            cfg.llm_profiles.push(profile);
        }
        cfg.active_llm = name;
        println!();
    }

    if !is_real_value(&cfg.speech.doubao.app_id) {
        println!("   {BOLD}语音供应商:{RESET} Doubao  {MUTED}(console.volcengine.com/speech){RESET}");
        cfg.speech.doubao.app_id = prompt_required("   App ID")?;
    }
    if !is_real_value(&cfg.speech.doubao.access_token) {
        cfg.speech.doubao.access_token = prompt_required("   Access Token")?;
    }

    cfg.save()?;
    println!();
    println!("   {BR_GREEN}✓{RESET}  配置已保存，运行 {BOLD}cb config{RESET} 进入完整设置");
    println!();
    Ok(cfg)
}

pub fn show() -> Result<()> {
    let cfg = AppConfig::load().unwrap_or_default();

    println!();
    println!(
        "   {BOLD}当前配置{RESET}  {MUTED}{CONFIG_PATH_DISPLAY}{RESET}"
    );
    println!();

    // Persona
    let lang_label = match cfg.persona.language.as_str() {
        "zh" => "中文",
        _ => "English",
    };
    println!("   {BR_CYAN}── Persona{RESET}");
    println!("   {MUTED}名称{RESET}      {BOLD}{}{RESET}", cfg.persona.name);
    println!("   {MUTED}语言{RESET}      {lang_label} ({})", cfg.persona.language);
    let ww = &cfg.persona.wake_word;
    println!(
        "   {MUTED}唤醒词{RESET}    {} {}",
        if ww.enabled {
            format!("{BR_GREEN}开启{RESET}")
        } else {
            format!("{MUTED}关闭{RESET}")
        },
        if ww.enabled {
            format!("{BOLD}「{}」{RESET}", ww.word)
        } else {
            String::new()
        }
    );
    println!();

    // LLM profiles
    println!("   {BR_CYAN}── LLM Profiles{RESET}");
    if cfg.llm_profiles.is_empty() {
        println!("   {MUTED}(未配置){RESET}");
    }
    for profile in &cfg.llm_profiles {
        let active = profile.name == cfg.active_llm || cfg.llm_profiles.len() == 1;
        let marker = if active {
            format!("{BR_CYAN}●{RESET}")
        } else {
            format!("{MUTED}○{RESET}")
        };
        let key_display = if profile.api_key.is_empty() || profile.api_key == "ollama" {
            if profile.api_key == "ollama" {
                format!("{MUTED}(local, no key){RESET}")
            } else {
                format!("{BR_RED}(未设置){RESET}")
            }
        } else {
            format!("{MUTED}{}{RESET}", mask_key(&profile.api_key))
        };
        println!(
            "   {marker} {BOLD}{:<12}{RESET}  {MUTED}{:<38}{RESET}  {}  {key_display}",
            profile.name, profile.base_url, profile.model
        );
    }
    println!();

    // Speech
    let voice_label = DOUBAO_VOICES
        .iter()
        .find(|v| v.id == cfg.speech.doubao.voice_type)
        .map(|v| format!("{} · {}", v.name, v.style))
        .unwrap_or_else(|| "Custom".to_string());
    println!("   {BR_CYAN}── 语音{RESET}");
    println!("   {MUTED}供应商{RESET}    Doubao");
    println!(
        "   {MUTED}App ID{RESET}    {}",
        if cfg.speech.doubao.app_id.is_empty() {
            format!("{BR_RED}(未设置){RESET}")
        } else {
            cfg.speech.doubao.app_id.clone()
        }
    );
    println!(
        "   {MUTED}Token{RESET}     {}",
        if cfg.speech.doubao.access_token.is_empty() {
            format!("{BR_RED}(未设置){RESET}")
        } else {
            format!("{MUTED}{}{RESET}", mask_key(&cfg.speech.doubao.access_token))
        }
    );
    println!(
        "   {MUTED}音色{RESET}      {BOLD}{}{RESET}  {MUTED}{}{RESET}",
        cfg.speech.doubao.voice_type, voice_label
    );
    println!("   {MUTED}语速{RESET}      {}x", cfg.speech.doubao.tts_speed);
    println!();

    println!(
        "   {MUTED}修改单项:  cb config set <key> <value>{RESET}"
    );
    println!("   {MUTED}完整向导:  cb config{RESET}");
    println!();
    Ok(())
}

pub fn set(key: &str, value: &str) -> Result<()> {
    let mut cfg = AppConfig::load().unwrap_or_default();

    match key {
        "persona.name" => cfg.persona.name = value.to_string(),
        "persona.language" | "locale" => {
            if value != "en" && value != "zh" {
                anyhow::bail!("language 必须是 'en' 或 'zh'");
            }
            cfg.persona.language = value.to_string();
        }
        "persona.wake_word.enabled" | "wake_word.enabled" => {
            cfg.persona.wake_word.enabled = matches!(value, "true" | "1" | "yes" | "on");
        }
        "persona.wake_word.word" | "wake_word.word" => {
            cfg.persona.wake_word.word = value.to_string();
        }
        "speech.doubao.app_id" => cfg.speech.doubao.app_id = value.to_string(),
        "speech.doubao.access_token" => cfg.speech.doubao.access_token = value.to_string(),
        "speech.doubao.voice_type" => cfg.speech.doubao.voice_type = value.to_string(),
        "speech.doubao.tts_speed" => {
            let n: f32 = value.parse().map_err(|_| anyhow::anyhow!("无效数字"))?;
            if !(0.5..=2.0).contains(&n) {
                anyhow::bail!("语速必须在 0.5 ~ 2.0 之间，当前值: {n}");
            }
            cfg.speech.doubao.tts_speed = n;
        }
        "audio.silence_seconds" => {
            cfg.audio.silence_seconds =
                value.parse().map_err(|_| anyhow::anyhow!("无效数字"))?;
        }
        "audio.min_speech_seconds" => {
            cfg.audio.min_speech_seconds =
                value.parse().map_err(|_| anyhow::anyhow!("无效数字"))?;
        }
        _ => anyhow::bail!(
            "未知 key: {key}\n\
             可用: persona.name, persona.language, persona.wake_word.enabled, persona.wake_word.word,\n\
             speech.doubao.app_id, speech.doubao.access_token, speech.doubao.voice_type,\n\
             speech.doubao.tts_speed, audio.silence_seconds, audio.min_speech_seconds"
        ),
    }

    cfg.save()?;
    println!("   {BR_GREEN}✓{RESET}  {key} = {BOLD}{value}{RESET}");
    Ok(())
}

// ─── Wizard steps ─────────────────────────────────────────────────────────────

fn wizard_persona(cfg: &mut AppConfig) -> Result<()> {
    print_step(1, 4, "语言 & 助手");

    // Language selection
    println!("   {MUTED}选择界面和对话语言:{RESET}");
    println!();
    let lang_opts = vec![
        SelectOption::new("中文", "zh").with_badge(
            if cfg.persona.language == "zh" { "● 当前" } else { "" },
        ),
        SelectOption::new("English", "en").with_badge(
            if cfg.persona.language == "en" { "● 当前" } else { "" },
        ),
    ];
    let lang_default = if cfg.persona.language == "zh" { 0 } else { 1 };
    if let Some(idx) = select::run(&lang_opts, lang_default)? {
        cfg.persona.language = if idx == 0 { "zh" } else { "en" }.to_string();
    }
    println!(
        "   {MUTED}语言: {}{RESET}",
        if cfg.persona.language == "zh" { "中文" } else { "English" }
    );
    println!();

    // Assistant name
    if let Some(v) = prompt_optional("   助手名称", &cfg.persona.name)? {
        cfg.persona.name = v;
    }
    println!();

    // Wake word
    println!("   {MUTED}唤醒词（关闭则随时说话都会响应）:{RESET}");
    println!();
    let ww_opts = vec![
        SelectOption::new("关闭", "随时响应").with_badge(
            if !cfg.persona.wake_word.enabled { "● 当前" } else { "" },
        ),
        SelectOption::new("开启", "说唤醒词后才响应").with_badge(
            if cfg.persona.wake_word.enabled { "● 当前" } else { "" },
        ),
    ];
    let ww_default = if cfg.persona.wake_word.enabled { 1 } else { 0 };
    if let Some(idx) = select::run(&ww_opts, ww_default)? {
        cfg.persona.wake_word.enabled = idx == 1;
    }
    if cfg.persona.wake_word.enabled {
        println!();
        if let Some(v) = prompt_optional("   唤醒词", &cfg.persona.wake_word.word)? {
            cfg.persona.wake_word.word = v;
        }
    }

    Ok(())
}

fn wizard_llm_profiles(cfg: &mut AppConfig) -> Result<()> {
    print_step(2, 4, "LLM 模型");

    loop {
        println!("   {MUTED}当前配置的模型 (● = 激活):{RESET}");
        println!();

        let mut opts: Vec<SelectOption> = cfg
            .llm_profiles
            .iter()
            .map(|p| {
                let hint = format!("{}", p.model);
                let badge = if p.name == cfg.active_llm || cfg.llm_profiles.len() == 1 {
                    "● 激活"
                } else {
                    ""
                };
                SelectOption::new(&p.name, hint).with_badge(badge)
            })
            .collect();
        opts.push(SelectOption::new("+ 添加新模型...", ""));

        let active_idx = cfg
            .llm_profiles
            .iter()
            .position(|p| p.name == cfg.active_llm)
            .unwrap_or(0);
        let default_idx = if cfg.llm_profiles.is_empty() {
            0
        } else {
            active_idx
        };

        let Some(choice) = select::run(&opts, default_idx)? else {
            break; // Esc = done
        };

        if choice == opts.len() - 1 {
            // Add new profile
            println!();
            if let Some(profile) = add_llm_profile(cfg)? {
                let name = profile.name.clone();
                cfg.llm_profiles.push(profile);
                cfg.active_llm = name;
                println!(
                    "   {BR_GREEN}✓{RESET}  已添加并激活"
                );
            }
        } else {
            // Set as active or edit existing
            let profile_name = cfg.llm_profiles[choice].name.clone();
            println!();
            println!(
                "   {MUTED}选中: {BOLD}{profile_name}{RESET}"
            );
            let action_opts = vec![
                SelectOption::new("激活此模型", "设为当前使用"),
                SelectOption::new("编辑配置", "修改 API key / 模型名"),
                SelectOption::new("删除", "从列表中移除"),
                SelectOption::new("← 返回", ""),
            ];
            if let Some(action) = select::run(&action_opts, 0)? {
                match action {
                    0 => {
                        cfg.active_llm = profile_name;
                        println!("   {BR_GREEN}✓{RESET}  已激活");
                    }
                    1 => {
                        println!();
                        edit_llm_profile(&mut cfg.llm_profiles[choice])?;
                        println!("   {BR_GREEN}✓{RESET}  已更新");
                    }
                    2 => {
                        if cfg.llm_profiles.len() == 1 {
                            println!("   {BR_YELLOW}⚠  至少保留一个模型{RESET}");
                        } else {
                            cfg.llm_profiles.remove(choice);
                            if cfg.active_llm == profile_name {
                                cfg.active_llm = cfg
                                    .llm_profiles
                                    .first()
                                    .map(|p| p.name.clone())
                                    .unwrap_or_default();
                            }
                            println!("   {MUTED}已删除{RESET}");
                        }
                    }
                    _ => {}
                }
            }
        }
        println!();
    }

    Ok(())
}

fn add_llm_profile(cfg: &AppConfig) -> Result<Option<LlmProfile>> {
    println!("   {MUTED}选择供应商:{RESET}");
    println!();

    let mut opts: Vec<SelectOption> = LLM_PRESETS
        .iter()
        .map(|p| {
            let already = cfg.llm_profiles.iter().any(|existing| existing.name == p.name);
            let badge = if already { "已配置" } else { "" };
            SelectOption::new(p.name, p.base_url).with_badge(badge)
        })
        .collect();
    opts.push(SelectOption::new("Custom...", "自定义 URL"));

    let Some(choice) = select::run(&opts, 0)? else {
        return Ok(None);
    };
    println!();

    let profile = if choice < LLM_PRESETS.len() {
        let preset = &LLM_PRESETS[choice];

        // Find existing or default model
        let current_model = cfg
            .llm_profiles
            .iter()
            .find(|p| p.name == preset.name)
            .map(|p| p.model.as_str())
            .unwrap_or(preset.default_model);

        println!(
            "   {MUTED}URL: {}{RESET}",
            preset.base_url
        );
        let model = prompt_optional("   模型", current_model)?.unwrap_or_else(|| current_model.to_string());
        println!();

        let api_key = if preset.needs_key {
            // Check if already have a key for this provider
            let current_key = cfg
                .llm_profiles
                .iter()
                .find(|p| p.name == preset.name)
                .map(|p| mask_key(&p.api_key))
                .unwrap_or_default();

            if current_key.is_empty() {
                prompt_required(&format!("   {} API Key", preset.name))?
            } else {
                prompt_optional("   API Key", &current_key)?.map(|_| {
                    // User entered new key
                    String::new() // placeholder, see below
                }).unwrap_or_else(|| {
                    // Keep existing
                    cfg.llm_profiles
                        .iter()
                        .find(|p| p.name == preset.name)
                        .map(|p| p.api_key.clone())
                        .unwrap_or_default()
                })
            }
        } else {
            "ollama".to_string()
        };

        LlmProfile {
            name: preset.name.to_string(),
            base_url: preset.base_url.to_string(),
            model,
            api_key,
        }
    } else {
        let name = prompt_required("   名称（如 My DeepSeek）")?;
        let base_url = prompt_required("   Base URL")?;
        let model = prompt_required("   模型")?;
        let api_key = prompt_required("   API Key")?;
        LlmProfile {
            name,
            base_url,
            model,
            api_key,
        }
    };

    Ok(Some(profile))
}

fn edit_llm_profile(profile: &mut LlmProfile) -> Result<()> {
    if let Some(v) = prompt_optional("   模型", &profile.model)? {
        profile.model = v;
    }
    let masked = mask_key(&profile.api_key);
    if let Some(v) = prompt_optional("   API Key", &masked)? {
        if v != masked {
            profile.api_key = v;
        }
    }
    Ok(())
}

fn wizard_speech(cfg: &mut AppConfig) -> Result<()> {
    print_step(3, 4, "语音供应商");

    println!("   {BOLD}Doubao{RESET}  {MUTED}(字节跳动 BigASR + TTS){RESET}");
    println!("   {MUTED}控制台: console.volcengine.com/speech{RESET}");
    println!();

    if is_real_value(&cfg.speech.doubao.app_id) {
        if let Some(v) = prompt_optional("   App ID", &cfg.speech.doubao.app_id)? {
            cfg.speech.doubao.app_id = v;
        }
    } else {
        cfg.speech.doubao.app_id = prompt_required("   App ID")?;
    }

    let masked = mask_key(&cfg.speech.doubao.access_token);
    if is_real_value(&cfg.speech.doubao.access_token) {
        if let Some(v) = prompt_optional("   Access Token", &masked)? {
            if v != masked {
                cfg.speech.doubao.access_token = v;
            }
        }
    } else {
        cfg.speech.doubao.access_token = prompt_required("   Access Token")?;
    }

    Ok(())
}

fn wizard_voice(cfg: &mut AppConfig) -> Result<()> {
    print_step(4, 4, "音色");

    let current_idx = DOUBAO_VOICES
        .iter()
        .position(|v| v.id == cfg.speech.doubao.voice_type);

    println!("   {MUTED}选择 TTS 音色:{RESET}");
    println!();

    let mut opts: Vec<SelectOption> = DOUBAO_VOICES
        .iter()
        .map(|v| {
            let badge = if Some(v.id) == current_idx.map(|i| DOUBAO_VOICES[i].id) {
                "● 当前"
            } else {
                ""
            };
            SelectOption::new(
                format!("{:<8}  {}", v.name, v.style),
                v.id,
            )
            .with_badge(badge)
        })
        .collect();
    opts.push(SelectOption::new("Custom...", "输入自定义 Voice ID"));

    let default_idx = current_idx.unwrap_or(0);
    if let Some(choice) = select::run(&opts, default_idx)? {
        if choice < DOUBAO_VOICES.len() {
            cfg.speech.doubao.voice_type = DOUBAO_VOICES[choice].id.to_string();
            println!(
                "   {MUTED}音色: {} ({}){RESET}",
                DOUBAO_VOICES[choice].name, DOUBAO_VOICES[choice].id
            );
        } else {
            println!();
            cfg.speech.doubao.voice_type = prompt_required("   Voice ID")?;
        }
    }

    println!();
    let speed = cfg.speech.doubao.tts_speed;
    loop {
        println!("   {MUTED}范围: 0.5（最慢）~ 2.0（最快），推荐 1.0~1.5{RESET}");
        match prompt_optional("   语速", &format!("{speed:.1}"))? {
            None => break, // 回车保留当前值
            Some(v) => match v.parse::<f32>() {
                Ok(n) if (0.5..=2.0).contains(&n) => {
                    cfg.speech.doubao.tts_speed = n;
                    break;
                }
                Ok(_) => println!("   {BR_YELLOW}⚠  请输入 0.5 ~ 2.0 之间的数值{RESET}"),
                Err(_) => println!("   {BR_YELLOW}⚠  无效数字，请重新输入{RESET}"),
            },
        }
    }

    Ok(())
}

// ─── UI helpers ───────────────────────────────────────────────────────────────

fn print_wizard_header() {
    println!();
    println!("   {BR_CYAN}╭─────────────────────────────────────────────╮{RESET}");
    println!("   {BR_CYAN}│{RESET}  {BOLD}cb  配置向导{RESET}                             {BR_CYAN}│{RESET}");
    println!("   {BR_CYAN}│{RESET}  {MUTED}{CONFIG_PATH_DISPLAY:<43}{RESET}  {BR_CYAN}│{RESET}");
    println!("   {BR_CYAN}╰─────────────────────────────────────────────╯{RESET}");
}

fn print_step(n: usize, total: usize, title: &str) {
    let dashes = "─".repeat(30_usize.saturating_sub(title.chars().count()));
    println!();
    println!("   {BR_CYAN}── Step {n}/{total}  {BOLD}{title}{RESET}  {BR_CYAN}{dashes}{RESET}");
    println!();
}

fn prompt_required(label: &str) -> Result<String> {
    loop {
        print!("{label}: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
        println!("   {MUTED}(必填){RESET}");
    }
}

fn prompt_optional(label: &str, current: &str) -> Result<Option<String>> {
    if !current.is_empty() {
        println!("   {MUTED}↵ 回车保留当前值 · 输入新值覆盖{RESET}");
    }
    print!("{label} [{MUTED}{current}{RESET}]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

fn mask_key(key: &str) -> String {
    if key.is_empty() || key == "ollama" {
        return key.to_string();
    }
    if key.len() <= 8 {
        "*".repeat(key.len())
    } else {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}
