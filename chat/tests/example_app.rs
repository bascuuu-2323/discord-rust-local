use reqwest::blocking::Client;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tonic_health::pb::{
    HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
};

struct TestServer(Child);
impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn example_app_callbacks_and_grpc_health() {
    let key = Ed25519KeyPair::from_seed_unchecked(&[42; 32]).unwrap();
    let mut server = TestServer(
        Command::new(env!("CARGO_BIN_EXE_discord-server"))
            .env("DISCORD_LOCAL_PORT", "0")
            .env("DISCORD_LOCAL_DB", ":memory:")
            .env(
                "DISCORD_INTERACTIONS_PUBLIC_KEY",
                hex::encode(key.public_key().as_ref()),
            )
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut root = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut root)
        .unwrap();
    let root = root.trim();
    assert!(root.starts_with("http://127.0.0.1:"));

    let http = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let timestamp = "1795766400";
    let submit = |signed_body: &str, actual_body: &str| {
        let signature = key.sign(format!("{timestamp}{signed_body}").as_bytes());
        http.post(format!("{root}/interactions"))
            .header("X-Signature-Timestamp", timestamp)
            .header("X-Signature-Ed25519", hex::encode(signature.as_ref()))
            .body(actual_body.to_owned())
            .send()
            .unwrap()
    };
    let ping = r#"{ "type": 1 }"#;
    let pong = submit(ping, ping);
    assert_eq!(pong.json::<Value>().unwrap(), json!({"type":1}));
    assert_eq!(submit(ping, r#"{"type":1}"#).status(), 401);
    assert_eq!(
        http.post(format!("{root}/interactions"))
            .body(ping)
            .send()
            .unwrap()
            .status(),
        401
    );
    assert_eq!(submit("{", "{").status(), 400);

    let test = r#"{"type":2,"data":{"name":"test"}}"#;
    let result: Value = submit(test, test).json().unwrap();
    assert_eq!(result["type"], 4);
    assert_eq!(result["data"]["flags"], 32768);
    assert_eq!(result["data"]["components"][0]["type"], 10);
    assert!(
        result["data"]["components"][0]["content"]
            .as_str()
            .unwrap()
            .starts_with("hello world ")
    );

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let channel = tonic::transport::Endpoint::from_shared(root.to_owned())
            .unwrap()
            .connect_timeout(Duration::from_secs(5))
            .connect()
            .await
            .unwrap();
        let mut grpc = HealthClient::new(channel);
        for service in ["", "discord.local.chat"] {
            let mut request = tonic::Request::new(HealthCheckRequest {
                service: service.into(),
            });
            request.set_timeout(Duration::from_secs(5));
            assert_eq!(
                grpc.check(request).await.unwrap().into_inner().status(),
                ServingStatus::Serving
            );
        }
        let mut request = tonic::Request::new(HealthCheckRequest {
            service: "missing".into(),
        });
        request.set_timeout(Duration::from_secs(5));
        assert_eq!(
            grpc.check(request).await.unwrap_err().code(),
            tonic::Code::NotFound
        );
        let mut request = tonic::Request::new(HealthCheckRequest {
            service: "discord.local.chat".into(),
        });
        request.set_timeout(Duration::from_secs(5));
        let mut stream = grpc.watch(request).await.unwrap().into_inner();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), stream.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .status(),
            ServingStatus::Serving
        );
    });
}
