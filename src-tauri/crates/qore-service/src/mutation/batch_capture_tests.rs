// SPDX-License-Identifier: BUSL-1.1

use super::*;

#[tokio::test]
async fn batch_captures_each_database_state_before_the_next_write() {
    let f = fixture(false, false, true).await;
    f.driver.execute(f.session,
        "CREATE TRIGGER normalize AFTER UPDATE OF name ON items BEGIN UPDATE items SET name = upper(NEW.name) WHERE id = NEW.id; END",
        QueryId::new()).await.unwrap();
    let update = |name: &str| SandboxChangeDto {
        change_type: SandboxChangeType::Update,
        primary_key: Some(RowData::new().with_column("id", Value::Int(1))),
        old_values: Some(
            RowData::new()
                .with_column("name", Value::Text("stale UI image".into()))
                .columns,
        ),
        new_values: Some(
            RowData::new()
                .with_column("name", Value::Text(name.into()))
                .columns,
        ),
        ..insert(&f, 1)
    };
    let changes = [
        insert(&f, 1),
        update("second"),
        update("third"),
        delete(&f, 1),
    ];
    let result = apply_batch_with_capture(
        &f.manager,
        &f.interceptor,
        &crate::cache::QueryCache::new(),
        f.session,
        &changes,
        true,
        false,
        &[0, 1, 2, 3],
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.captures.len(), 4);
    let image = |index: usize, before: bool| {
        let capture = &result.captures[index].1;
        let row = if before {
            &capture.before
        } else {
            &capture.after
        };
        serde_json::to_value(&row.as_ref().unwrap().columns["name"]).unwrap()
    };
    assert_eq!(image(0, false), "fixture");
    assert_eq!(image(1, true), "fixture");
    assert_eq!(image(1, false), "SECOND");
    assert_eq!(image(2, true), "SECOND");
    assert_eq!(image(2, false), "THIRD");
    assert_eq!(image(3, true), "THIRD");
    assert!(result.captures[3].1.after.is_none());
    assert_eq!(count(&f).await, 0);
    let wire = serde_json::to_string(&result).unwrap();
    assert!(!wire.contains("captures"));
    assert!(
        !wire.contains("THIRD"),
        "internal unmasked images must never reach IPC"
    );
}

#[tokio::test]
async fn lost_commit_response_discards_even_readable_images() {
    let f = fixture_with_commit_failure(CommitFailure::AfterCommit, false, true).await;
    let result = apply_batch_with_capture(
        &f.manager,
        &f.interceptor,
        &crate::cache::QueryCache::new(),
        f.session,
        &[insert(&f, 1)],
        true,
        false,
        &[0],
    )
    .await;
    assert!(result.outcome_unknown);
    assert_eq!(count(&f).await, 1);
    assert!(result.captures.is_empty());
    assert!(result.applied_indices.is_empty());
}

#[tokio::test]
async fn nontransactional_failure_keeps_only_confirmed_images() {
    let f = fixture(false, false, false).await;
    let result = apply_batch_with_capture(
        &f.manager,
        &f.interceptor,
        &crate::cache::QueryCache::new(),
        f.session,
        &[insert(&f, 1), insert(&f, 1), insert(&f, 3)],
        false,
        false,
        &[0, 1, 2],
    )
    .await;
    assert!(!result.success);
    assert_eq!(result.captures.len(), 1);
    assert_eq!(result.captures[0].0, 0);
    assert!(result.captures[0].1.after.is_some());
}

#[tokio::test]
async fn capture_is_opt_in_and_does_not_bypass_mutation_guards() {
    let f = fixture(false, false, true).await;
    let result = guarded(&f, &[insert(&f, 1)], false).await;
    assert!(result.success);
    assert!(result.captures.is_empty());
    let mut config = f.config.clone();
    config.read_only = true;
    let read_only_session = f.manager.connect(config).await.unwrap();
    let result = apply_batch_with_capture(
        &f.manager,
        &f.interceptor,
        &crate::cache::QueryCache::new(),
        read_only_session,
        &[insert(&f, 2)],
        true,
        false,
        &[0],
    )
    .await;
    assert!(!result.success);
    assert!(result.captures.is_empty());
    assert_eq!(count(&f).await, 1);
}

#[tokio::test]
async fn batch_bounds_retained_images_without_losing_confirmed_writes() {
    let f = fixture(false, false, true).await;
    f.driver
        .execute(
            f.session,
            "INSERT INTO items (id, name) VALUES (1, hex(zeroblob(750000)))",
            QueryId::new(),
        )
        .await
        .unwrap();
    let changes: Vec<_> = (0..4)
        .map(|_| SandboxChangeDto {
            change_type: SandboxChangeType::Update,
            primary_key: Some(RowData::new().with_column("id", Value::Int(1))),
            old_values: None,
            new_values: Some(
                RowData::new()
                    .with_column("name", Value::Text("a".repeat(1_500_000)))
                    .columns,
            ),
            ..insert(&f, 1)
        })
        .collect();
    let result = apply_batch_with_capture(
        &f.manager,
        &f.interceptor,
        &crate::cache::QueryCache::new(),
        f.session,
        &changes,
        true,
        false,
        &[0, 1, 2, 3],
    )
    .await;
    assert!(result.success);
    assert_eq!(result.applied_count, 4);
    assert_eq!(result.captures.len(), 2);
    let retained: usize = result
        .captures
        .iter()
        .map(|(_, capture)| capture.retained_bytes(MAX_CAPTURE_BYTES).unwrap())
        .sum();
    assert!(retained <= MAX_CAPTURE_BYTES);
}

#[tokio::test]
async fn batch_generated_keys_follow_commit_and_rollback() {
    for use_transaction in [false, true] {
        let f = fixture(false, false, true).await;
        let mut generated = insert(&f, 1);
        generated.new_values.as_mut().unwrap().remove("id");
        let result = apply_batch_with_capture(
            &f.manager,
            &f.interceptor,
            &crate::cache::QueryCache::new(),
            f.session,
            &[generated, insert(&f, 1)],
            use_transaction,
            false,
            &[0, 1],
        )
        .await;
        assert!(!result.success);
        assert_eq!(result.outcome_unknown, !use_transaction);
        if use_transaction {
            assert!(result.captures.is_empty());
            assert_eq!(count(&f).await, 0);
        } else {
            assert_eq!(count(&f).await, 1);
            assert_eq!(result.captures.len(), 1);
            let capture = &result.captures[0].1;
            assert_eq!(
                serde_json::to_value(&capture.primary_key.columns["id"]).unwrap(),
                1
            );
            assert!(capture.after.is_some());
            assert!(!serde_json::to_string(&result).unwrap().contains("fixture"));
        }
    }
}
