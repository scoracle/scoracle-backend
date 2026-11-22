#[tokio::test]
#[ignore = "requires SCORACLE_MEMORY_STUDY_BIN"]
async fn installed_helper_handles_nonempty_startup_history() {
    super::verify_runtime().await.unwrap();
}
