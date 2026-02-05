use qoredb_lib::AppState;

#[tokio::test]
async fn test_app_state_initialization() {

    let state = AppState::new();

    let drivers = state.registry.list();
    assert!(drivers.contains(&"postgres"), "Postgres driver should be registered");
    assert!(drivers.contains(&"mysql"), "MySQL driver should be registered");
    assert!(drivers.contains(&"mongodb"), "MongoDB driver should be registered");


    let _policy = &state.policy;
    assert!(state.session_manager.list_sessions().await.is_empty(), "Session manager should start empty");
}
