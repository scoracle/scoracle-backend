//! Export one current character JSON package without a database or model call.
use anyhow::{ensure, Context, Result};
use scoracle_cognition::evaluation::tasks::{resolve_task, Fixture};

fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("usage: character_package FIXTURE.json")?;
    let fixture: Fixture = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let task = resolve_task(&fixture.task).context("unknown character task")?;
    ensure!(
        fixture.prompt_version == task.prompt_version(),
        "fixture uses an older prompt contract"
    );
    let request = match &fixture.parts {
        Some(parts) => task.assemble(parts)?,
        None => task.prepare_fixture(&fixture)?,
    };
    if std::env::args().nth(2).as_deref() == Some("--request") {
        let options = request.options;
        println!(
            "{}",
            serde_json::json!({"user_prompt":request.user_prompt,
            "should_call":request.should_call,"system":options.system,
            "temperature":options.temperature,"num_predict":options.num_predict,
            "num_ctx":options.num_ctx,"json_mode":options.json_mode,
            "format_schema":options.format_schema,"format_schema_raw":options.format_schema_raw})
        );
    } else {
        print!("{}", request.user_prompt);
    }
    Ok(())
}
