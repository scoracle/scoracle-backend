use anyhow::Result;
use scoracle_cognition::harness::{
    model::{GenerateOptions, Inference},
    providers::llama::LlamaClient,
};
use serde_json::{json, Value};
use std::time::Duration;

#[tokio::test]
#[ignore = "local_real_model: requires TEST_LLAMA_URL and a native SmolLM3 server"]
async fn local_real_model_preserves_input_and_completes_a_character_request() -> Result<()> {
    let client = LlamaClient::new(
        &std::env::var("TEST_LLAMA_URL")?,
        scoracle_cognition::harness::config::DEFAULT_MODEL,
        Duration::from_secs(60),
        Some(false),
    )?;
    let opts = GenerateOptions {
        system: Some("Return only a JSON object using the supplied value.".into()),
        num_ctx: 32768,
        num_predict: 128,
        temperature: Some(0.0),
        format_schema: Some(
            json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false}),
        ),
        ..Default::default()
    };
    let long_prompt = "Supplied publisher evidence. ".repeat(3000);
    let prepared = client.prepare(&long_prompt, &opts).await?;
    prepared.verify_input(&long_prompt, &opts)?;
    assert!(prepared.coverage.as_ref().unwrap().token_ids.len() > 8192);
    let (result, _) = client
        .generate(
            "The supplied value is Cedar FC. Copy it to the value field.",
            &opts,
        )
        .await?;
    assert_eq!(
        serde_json::from_str::<Value>(&result.response)?["value"],
        "Cedar FC"
    );
    assert_eq!(result.completion_reason.as_deref(), Some("eos"));
    assert!(result.prompt_eval_count > 0);
    Ok(())
}
