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
    let request = task.prepare_fixture(&fixture)?;
    print!("{}", request.user_prompt);
    Ok(())
}
