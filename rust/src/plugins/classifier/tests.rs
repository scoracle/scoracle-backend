use super::*;

#[tokio::test]
async fn acquisition_replay_requires_explicit_opt_in() {
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/classifier_test").unwrap();
    for setting in [None, Some("false"), Some(""), Some("TRUE"), Some("typo")] {
        assert!(adapter::acquisition_replay(&pool, setting).is_empty());
    }
    let operations = adapter::acquisition_replay(&pool, Some("true"));
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].name(), "classifier.acquisition-replay");
}
