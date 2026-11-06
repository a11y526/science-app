// GitHub 로그인(기기 코드 방식)으로 선생님의 science-portal을 찾거나 만든다.
// 로그인 열쇠(토큰)는 이 파일 안(앱 메모리)에만 두고 화면(JS)에는 내보내지 않는다. 저장하지 않는다.
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

// GitHub OAuth 앱(과학 관리)의 Client ID. 공개해도 되는 값. 비어 있으면 GitHub 로그인 단추를 숨긴다.
const CLIENT_ID: &str = "";
const UPSTREAM: &str = "a11y526/science-portal";
const REPO: &str = "science-portal";

#[derive(Default)]
pub struct Gh {
    device: Mutex<Option<String>>,
    token: Mutex<Option<String>>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("science-app")
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
}

async fn api(token: &str, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<(u16, Value), String> {
    let mut req = client()?
        .request(method, format!("https://api.github.com{path}"))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(b) = body {
        req = req.json(&b);
    }
    let res = req.send().await.map_err(|_| "NETWORK".to_string())?;
    let code = res.status().as_u16();
    let v = res.json::<Value>().await.unwrap_or(Value::Null);
    Ok((code, v))
}

fn token(state: &State<'_, Gh>) -> Result<String, String> {
    state.token.lock().unwrap().clone().ok_or_else(|| "NO_LOGIN".to_string())
}

async fn login_of(t: &str) -> Result<String, String> {
    let (code, user) = api(t, reqwest::Method::GET, "/user", None).await?;
    if code != 200 {
        return Err("NO_LOGIN".into());
    }
    user["login"].as_str().map(|s| s.to_string()).ok_or_else(|| "NO_LOGIN".into())
}

fn site_url(login: &str) -> String {
    format!("https://{}.github.io/{REPO}/", login.to_lowercase())
}


// 앱 안 첫 화면(dist/index.html)에서 부른 것만 받는다. 포털 화면에서는 못 부름.
fn local(w: &tauri::Webview) -> Result<(), String> {
    let u = w.url().map_err(|_| "NOT_LOCAL".to_string())?;
    if u.scheme() == "tauri" || u.host_str() == Some("tauri.localhost") {
        Ok(())
    } else {
        Err("NOT_LOCAL".into())
    }
}

#[tauri::command]
pub fn gh_ready() -> bool {
    !CLIENT_ID.is_empty()
}

// 기기 코드를 받아 GitHub 승인 화면을 브라우저로 연다. 화면에는 사람이 넣을 코드만 준다.
#[tauri::command]
pub async fn gh_start(webview: tauri::Webview, app: AppHandle, state: State<'_, Gh>) -> Result<Value, String> {
    local(&webview)?;
    let v: Value = client()?
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", CLIENT_ID), ("scope", "public_repo")])
        .send()
        .await
        .map_err(|_| "NETWORK".to_string())?
        .json()
        .await
        .map_err(|_| "NETWORK".to_string())?;
    let dc = v["device_code"].as_str().ok_or_else(|| "START".to_string())?.to_string();
    *state.device.lock().unwrap() = Some(dc);
    *state.token.lock().unwrap() = None;
    let uri = v["verification_uri"].as_str().unwrap_or("https://github.com/login/device").to_string();
    let _ = app.opener().open_url(uri.clone(), None::<&str>);
    Ok(json!({ "code": v["user_code"], "uri": uri, "interval": v["interval"].as_u64().unwrap_or(5) }))
}

#[tauri::command]
pub fn gh_open(webview: tauri::Webview, app: AppHandle, url: String) -> Result<(), String> {
    local(&webview)?;
    if !url.starts_with("https://github.com/") {
        return Err("BAD_URL".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

// 승인됐는지 묻는다: "ok" / "authorization_pending" / "slow_down" / "expired_token" / "access_denied"
#[tauri::command]
pub async fn gh_poll(webview: tauri::Webview, state: State<'_, Gh>) -> Result<String, String> {
    local(&webview)?;
    let dc = state.device.lock().unwrap().clone().ok_or_else(|| "NO_DEVICE".to_string())?;
    let v: Value = client()?
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", CLIENT_ID),
            ("device_code", dc.as_str()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await
        .map_err(|_| "NETWORK".to_string())?
        .json()
        .await
        .map_err(|_| "NETWORK".to_string())?;
    if let Some(t) = v["access_token"].as_str() {
        *state.token.lock().unwrap() = Some(t.to_string());
        *state.device.lock().unwrap() = None;
        return Ok("ok".into());
    }
    Ok(v["error"].as_str().unwrap_or("error").to_string())
}

// 내 science-portal이 있는지, Pages가 켜져 있는지
#[tauri::command]
pub async fn gh_find(webview: tauri::Webview, state: State<'_, Gh>) -> Result<Value, String> {
    local(&webview)?;
    let t = token(&state)?;
    let login = login_of(&t).await?;
    let (code, _) = api(&t, reqwest::Method::GET, &format!("/repos/{login}/{REPO}"), None).await?;
    if code != 200 {
        return Ok(json!({ "login": login, "repo": false, "pages": false, "url": site_url(&login) }));
    }
    let (pc, _) = api(&t, reqwest::Method::GET, &format!("/repos/{login}/{REPO}/pages"), None).await?;
    Ok(json!({ "login": login, "repo": true, "pages": pc == 200, "url": site_url(&login) }))
}

// 없으면 Fork, Pages가 꺼져 있으면 켠다. 사이트 주소를 돌려준다.
#[tauri::command]
pub async fn gh_create(webview: tauri::Webview, state: State<'_, Gh>) -> Result<String, String> {
    local(&webview)?;
    let t = token(&state)?;
    let login = login_of(&t).await?;
    let repo = format!("/repos/{login}/{REPO}");
    let (code, _) = api(&t, reqwest::Method::GET, &repo, None).await?;
    if code != 200 {
        let (fc, fv) = api(
            &t,
            reqwest::Method::POST,
            &format!("/repos/{UPSTREAM}/forks"),
            Some(json!({ "default_branch_only": true })),
        )
        .await?;
        if fc >= 300 {
            return Err(format!("FORK {fc} {}", fv["message"].as_str().unwrap_or("")));
        }
        // Fork는 몇 초 뒤에 만들어진다. main 가지가 생길 때까지 기다림(최대 2분)
        let mut ready = false;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let (bc, _) = api(&t, reqwest::Method::GET, &format!("{repo}/branches/main"), None).await?;
            if bc == 200 {
                ready = true;
                break;
            }
        }
        if !ready {
            return Err("FORK_SLOW".into());
        }
    }
    let (pc, _) = api(&t, reqwest::Method::GET, &format!("{repo}/pages"), None).await?;
    if pc != 200 {
        let (ec, ev) = api(
            &t,
            reqwest::Method::POST,
            &format!("{repo}/pages"),
            Some(json!({ "source": { "branch": "main", "path": "/" } })),
        )
        .await?;
        if ec >= 300 && ec != 409 {
            return Err(format!("PAGES {ec} {}", ev["message"].as_str().unwrap_or("")));
        }
    }
    Ok(site_url(&login))
}
