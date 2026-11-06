use axum::{
    Json, Router,
    extract::{Query, WebSocketUpgrade},
    http::{Method, StatusCode},
    routing::get,
};
use phone_server::{
    config::Config,
    controller::Control,
    gateway::{Ari, Caller, Channel},
    model::Phase,
    store::Store,
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

#[tokio::test]
async fn external_media_waits_for_its_own_stasis_event_before_bridge_attachment() {
    let (events, received) = tokio::sync::mpsc::channel::<String>(16);
    let received = Arc::new(Mutex::new(Some(received)));
    let connected = Arc::new(tokio::sync::Notify::new());
    let ready = Arc::new(AtomicBool::new(false));
    let attachments = Arc::new(AtomicUsize::new(0));
    let channel = Arc::new(Mutex::new(None::<String>));
    let external = channel.clone();
    let connected_media = connected.clone();
    let attached = attachments.clone();
    let entered = ready.clone();
    let id = uuid::Uuid::new_v4();
    let app=Router::new()
        .route("/ari/channels",get(||async{Json(serde_json::json!([]))}))
        .route("/ari/bridges",get(||async{Json(serde_json::json!([]))}))
        .route("/ari/applications/homephone",get(||async{Json(serde_json::json!({"channel_ids":[]}))}))
        .route("/ari/events",get(move|socket:WebSocketUpgrade|{
            let mut events=received.lock().unwrap().take().unwrap();
            async move{socket.on_upgrade(move|mut socket|async move{while let Some(event)=events.recv().await{if socket.send(axum::extract::ws::Message::Text(event.into())).await.is_err(){break;}}})}
        }))
        .route("/ari/channels/externalMedia",axum::routing::post(move|Query(query):Query<HashMap<String,String>>|{
            *external.lock().unwrap()=Some(query["channelId"].clone());
            async{StatusCode::NO_CONTENT}
        }))
        .route("/ari/channels/{id}/variable",get(move||async move{Json(serde_json::json!({"value":id.to_string()}))}))
        .route("/ari/bridges/{id}/addChannel",axum::routing::post(move|Query(query):Query<HashMap<String,String>>|{
            let media=query.get("channel").is_some_and(|value|value.contains("-media-"));
            let accepted=!media||entered.load(Ordering::SeqCst);
            if media {attached.fetch_add(1,Ordering::SeqCst);}
            async move{if accepted{StatusCode::NO_CONTENT}else{StatusCode::CONFLICT}}
        }))
        .route("/media/{id}",get(move|socket:WebSocketUpgrade|{
            let connected=connected_media.clone();
            async move{socket.on_upgrade(move|mut socket|async move{
                socket.send(axum::extract::ws::Message::Text(serde_json::json!({"event":"MEDIA_START","format":"ulaw","optimal_frame_size":160,"ptime":20}).to_string().into())).await.unwrap();
                connected.notify_one();
                while socket.recv().await.is_some(){}
            })}
        }))
        .fallback(axum::routing::any(|_method:Method|async{StatusCode::NO_CONTENT}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(&root.path().join("phone.sqlite"))
        .await
        .unwrap();
    let mut config = Config {
        data_dir: root.path().into(),
        ..Config::default()
    };
    config.ari.url = format!("http://{address}/ari");
    let ari = Ari::new(config.ari.clone(), "test-secret".into()).unwrap();
    let control = Control::start(config, ari, store.clone());
    control.wait_ready().await.unwrap();
    let offered = control.clone();
    let offering = tokio::spawn(async move {
        offered
            .offer(
                Channel {
                    id: "external-call".into(),
                    state: "Ring".into(),
                    caller: Caller {
                        number: "999005".into(),
                    },
                },
                "inbound".into(),
            )
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), connected.notified())
        .await
        .unwrap();
    let wrong = serde_json::json!({"type":"StasisStart","channel":{"id":"unrelated-generation","state":"Up"},"args":[]});
    events.send(wrong.to_string()).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let premature = attachments.load(Ordering::SeqCst);
    let media = channel.lock().unwrap().clone().unwrap();
    ready.store(true, Ordering::SeqCst);
    events
        .send(
            serde_json::json!({"type":"StasisStart","channel":{"id":media,"state":"Up"},"args":[]})
                .to_string(),
        )
        .await
        .unwrap();
    let result = offering.await.unwrap();
    control.shutdown().await;
    server.abort();
    assert_eq!(
        premature, 0,
        "a successful create response does not establish Stasis readiness"
    );
    assert_eq!(result.unwrap().phase, Phase::Screening);
    assert_eq!(attachments.load(Ordering::SeqCst), 1);
}
