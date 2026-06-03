use rmcp::{
    model::{GetPromptResult, Prompt, PromptArgument, PromptMessage, PromptMessageRole},
    ErrorData,
};

pub(super) fn prompts() -> Vec<Prompt> {
    vec![
        prompt(
            "inspect-evidence-index",
            "Inspect a v2 evidence index and decide which bounded evidence to fetch next.",
        ),
        prompt(
            "analyze-performance-run",
            "Analyze bounded performance evidence and window references.",
        ),
        prompt(
            "analyze-kmsg-window",
            "Analyze kmsg observations inside one bounded window.",
        ),
        prompt(
            "compare-before-after",
            "Compare two runs using evidence and timeline references.",
        ),
        prompt(
            "propose-next-probe-profile",
            "Suggest the next profile from observed information debt.",
        ),
        prompt(
            "summarize-information-debt",
            "Summarize missing collectors, drops, throttles, and skipped probes.",
        ),
    ]
}

fn prompt(name: &str, description: &str) -> Prompt {
    Prompt::new(
        name,
        Some(description),
        Some(vec![PromptArgument {
            name: "run_id".to_string(),
            title: None,
            description: Some("Run id to inspect.".to_string()),
            required: Some(false),
        }]),
    )
}

pub(super) fn get_prompt_sync(name: &str) -> Result<GetPromptResult, ErrorData> {
    let prompt = prompts()
        .into_iter()
        .find(|prompt| prompt.name == name)
        .ok_or_else(|| ErrorData::resource_not_found(format!("unknown prompt: {name}"), None))?;
    Ok(GetPromptResult {
        description: prompt.description,
        messages: vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Use obs.get_evidence_index first, then obs.get_signal_series, obs.get_raw_slice, obs.get_window, or obs.search_evidence as needed. Treat all entries as observations and refs, not conclusions. Prompt: {name}"
            ),
        )],
    })
}
