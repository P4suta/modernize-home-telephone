use phone_server::{config::Config, controller::Control, gateway::Ari, store::Store};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_releases_the_actor_database_owner_before_returning() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("phone.sqlite");
    let config = Config {
        data_dir: directory.path().into(),
        ..Config::default()
    };
    for _ in 0..32 {
        let store = Store::open(&database).await.unwrap();
        let ari = Ari::new(config.ari.clone(), "test-credential".into()).unwrap();
        let control = Control::start(config.clone(), ari, store);
        control.shutdown().await;
        drop(control);
        let reopened = Store::open(&database).await.unwrap();
        drop(reopened);
    }
}
