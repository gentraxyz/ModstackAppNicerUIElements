use tauri::command;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use reqwest::Client;
use minecraft_msa_auth::MinecraftAuthorizationFlow;

const CLIENT_ID: &str = "28345b95-0610-4565-b77d-03a20a541560";
const REDIRECT_URI: &str = "http://localhost:7878/callback";

#[command]
pub async fn login_microsoft() -> Result<Value, String> {
    let listener = TcpListener::bind("127.0.0.1:7878")
        .await
        .map_err(|e| e.to_string())?;

    let url = format!(
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize\
        ?client_id={}\
        &response_type=code\
        &redirect_uri={}\
        &scope=XboxLive.signin%20offline_access\
        &prompt=select_account",
        CLIENT_ID,
        REDIRECT_URI
    );

    open::that(url).map_err(|e| e.to_string())?;

    let code = loop {
        let (mut stream, _) = listener.accept().await.map_err(|e| e.to_string())?;

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
        let request = String::from_utf8_lossy(&buf[..n]);

        if let Some(line) = request.lines().next() {
            if line.contains("/callback") {
                if let Some(code_part) = line.split("code=").nth(1) {
                    let code = code_part
                        .split('&')
                        .next()
                        .unwrap_or("")
                        .split(' ')
                        .next()
                        .unwrap_or("")
                        .to_string();

                    let body = r#"<!DOCTYPE html>
                    <html lang="es">
                    <head>
                    <meta charset="UTF-8" />
                    <title>Modstack Auth Success</title>
                    <style>
                      body {
                        margin: 0;
                        background: #121212;
                        color: white;
                        font-family: Helvetica, Arial, sans-serif;
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        height: 100vh;
                      }
                      .card { text-align: center; }
                      .check {
                        width: 72px; height: 72px; border-radius: 50%;
                        background: #1ed760;
                        display: flex; align-items: center; justify-content: center;
                        margin: 0 auto 20px; font-size: 30px; color: black;
                      }
                      h1 { font-size: 24px; margin: 0; }
                      p { color: #aaa; margin-top: 10px; }
                    </style>
                    </head>
                    <body>
                      <div class="card">
                        <div class="check">&#10004;</div>
                        <h1>Authentication complete</h1>
                        <p>You can close this window</p>
                      </div>
                      <script>setTimeout(() => window.close(), 1500);</script>
                    </body>
                    </html>"#;

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );

                    let _ = stream.write_all(response.as_bytes()).await;
                    break code;
                }
            }
        }
    };

    let (ms_access_token, ms_refresh_token) = exchange_code(&code).await?;

    let mc_flow = MinecraftAuthorizationFlow::new(reqwest::Client::new());
    let mc_token = mc_flow
        .exchange_microsoft_token(&ms_access_token)
        .await
        .map_err(|e| e.to_string())?;

    let profile = get_minecraft_profile(mc_token.access_token().as_ref()).await?;

    Ok(json!({
        "type": "microsoft",
        "minecraft": {
            "name": profile["name"],
            "uuid": profile["id"],
            "access_token": mc_token.access_token().as_ref(),
            "refresh_token": ms_refresh_token,
            "ms_access_token": ms_access_token
        }
    }))
}

#[command]
pub async fn refresh_microsoft_token(refresh_token: String) -> Result<Value, String> {
    let client = Client::new();

    let params = [
        ("client_id", CLIENT_ID),
        ("refresh_token", refresh_token.as_str()),
        ("grant_type", "refresh_token"),
        ("scope", "XboxLive.signin offline_access"),
    ];

    let res = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: Value = res.json().await.map_err(|e| e.to_string())?;

    if let Some(err) = json.get("error") {
        return Err(format!("Error refresh: {}", err));
    }

    let new_ms_access = json["access_token"]
        .as_str()
        .ok_or("No access_token")?
        .to_string();
    let new_ms_refresh = json["refresh_token"]
        .as_str()
        .ok_or("No refresh_token")?
        .to_string();

    let mc_flow = MinecraftAuthorizationFlow::new(reqwest::Client::new());
    let mc_token = mc_flow
        .exchange_microsoft_token(&new_ms_access)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({
        "access_token": mc_token.access_token().as_ref(),
        "refresh_token": new_ms_refresh,
        "ms_access_token": new_ms_access
    }))
}

async fn exchange_code(code: &str) -> Result<(String, String), String> {
    let client = Client::new();

    let params = [
        ("client_id", CLIENT_ID),
        ("code", code),
        ("grant_type", "authorization_code"),
        ("redirect_uri", REDIRECT_URI),
    ];

    let res = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: Value = res.json().await.map_err(|e| e.to_string())?;

    let access_token = json["access_token"]
        .as_str()
        .ok_or("No access_token")?
        .to_string();
    let refresh_token = json["refresh_token"]
        .as_str()
        .ok_or("No refresh_token")?
        .to_string();

    Ok((access_token, refresh_token))
}

async fn get_minecraft_profile(access_token: &str) -> Result<Value, String> {
    let client = reqwest::Client::new();

    let response = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("Error perfil: {}", response.status()));
    }

    let profile: Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(profile)
}

#[command]
pub async fn login_offline(username: String) -> Result<Value, String> {
    Ok(json!({
        "type": "offline",
        "minecraft": {
            "name": username,
            "uuid": "00000000-0000-0000-0000-000000000000"
        }
    }))
}

#[command]
pub fn logout() {
    println!("Sesion cerrada");
}