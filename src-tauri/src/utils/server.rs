use super::resolve;
use crate::{
    cmd::{self, is_port_in_use},
    config::{Config, DEFAULT_PAC, IProfiles, IVerge},
    core::CoreManager,
    module::lightweight,
    process::AsyncHandler,
    service,
    utils::{dirs, window_manager::WindowManager},
};
use anyhow::{Result, bail};
use clash_verge_cli_protocol::{CliRequest, CliResponse, TOKEN_FILE};
use clash_verge_logging::{Type, logging, logging_error};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use reqwest::ClientBuilder;
use serde_json::{Value, json};
use smartstring::alias::String;
use std::{fs, path::PathBuf, time::Duration};
use tokio::sync::oneshot;
use warp::Filter as _;

#[derive(serde::Deserialize, Debug)]
struct QueryParam {
    param: String,
}

// 关闭 embedded server 的信号发送端
static SHUTDOWN_SENDER: OnceCell<Mutex<Option<oneshot::Sender<()>>>> = OnceCell::new();

fn cli_token_path() -> Result<PathBuf> {
    Ok(dirs::app_home_dir()?.join(TOKEN_FILE))
}

fn generate_cli_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn ensure_cli_token() -> Result<String> {
    let path = cli_token_path()?;
    if let Ok(token) = fs::read_to_string(&path) {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(token.into());
        }
    }

    let token = generate_cli_token()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, token.as_bytes())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(token)
}

fn authorized(authorization: Option<&str>, token: &str) -> bool {
    authorization
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|value| value == token)
}

async fn cli_status() -> Result<Value> {
    let verge = service::verge::get_config().await?;
    let clash_info = service::clash::get_info().await;
    let profiles = service::profile::get_config().await;
    let current_uid = profiles.current.clone();
    let current_name = current_uid
        .as_ref()
        .and_then(|uid| profiles.get_name_by_uid(uid))
        .cloned();

    Ok(json!({
        "app": {
            "running": true,
        },
        "core": {
            "running_mode": CoreManager::global().get_running_mode().to_string(),
            "info": {
                "mixed_port": clash_info.mixed_port,
                "socks_port": clash_info.socks_port,
                "port": clash_info.port,
                "server": clash_info.server,
            },
        },
        "verge": {
            "system_proxy": verge.enable_system_proxy.unwrap_or(false),
            "tun": verge.enable_tun_mode.unwrap_or(false),
            "external_controller_enabled": verge.enable_external_controller.unwrap_or(false),
        },
        "profile": {
            "uid": current_uid,
            "name": current_name,
        },
    }))
}

#[allow(clippy::cognitive_complexity)]
async fn dispatch_cli_request(request: &CliRequest) -> Result<Value> {
    match request.method.as_str() {
        "status" => cli_status().await,
        "core.restart" => {
            service::clash::restart().await?;
            Ok(json!({ "changed": true }))
        }
        "core.start" => {
            service::clash::start().await?;
            Ok(json!({ "changed": true }))
        }
        "core.stop" => {
            service::clash::stop().await?;
            Ok(json!({ "changed": true }))
        }
        "core.mode" => {
            let mode = request
                .params
                .get("mode")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("missing string parameter: mode"))?;
            if !matches!(mode, "rule" | "global" | "direct") {
                bail!("invalid core mode: {mode}");
            }
            service::clash::patch_mode(mode.into()).await;
            Ok(json!({ "changed": true, "mode": mode }))
        }
        "verge.patch" => {
            let patch = serde_json::from_value::<IVerge>(request.params.clone())?;
            service::verge::patch_config(&patch).await?;
            Ok(json!({ "changed": true }))
        }
        "profile.switch" => {
            let id_or_name = required_string(&request.params, "profile")?;
            let profile_id = service::profile::resolve_id(id_or_name).await?;
            let outcome = cmd::patch_profiles_config(IProfiles {
                current: Some(profile_id.clone()),
                items: None,
            })
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
            if !outcome.is_valid() {
                bail!("profile switch validation failed: {outcome}");
            }
            Ok(json!({
                "changed": true,
                "profile": profile_id,
                "validation": outcome,
            }))
        }
        "profile.update" => {
            let profile_id = service::profile::resolve_id(required_string(&request.params, "profile")?).await?;
            service::profile::update(&profile_id, None).await?;
            Ok(json!({ "changed": true, "profile": profile_id }))
        }
        "profile.delete" => {
            let profile_id = service::profile::resolve_id(required_string(&request.params, "profile")?).await?;
            cmd::delete_profile(profile_id.clone())
                .await
                .map_err(|error| anyhow::anyhow!(error))?;
            Ok(json!({ "changed": true, "profile": profile_id }))
        }
        "profile.read" => {
            let profile_id = service::profile::resolve_id(required_string(&request.params, "profile")?).await?;
            let content = cmd::read_profile_file(profile_id)
                .await
                .map_err(|error| anyhow::anyhow!(error))?;
            Ok(json!({ "content": content }))
        }
        "backup.create" => {
            service::backup::create().await?;
            Ok(json!({ "changed": true }))
        }
        "backup.list" => Ok(serde_json::to_value(service::backup::list().await?)?),
        "backup.delete" => {
            service::backup::delete(required_string(&request.params, "filename")?.into()).await?;
            Ok(json!({ "changed": true }))
        }
        "backup.restore" => {
            service::backup::restore(required_string(&request.params, "filename")?.into()).await?;
            Ok(json!({ "changed": true }))
        }
        "backup.import" => {
            let filename = service::backup::import(required_string(&request.params, "source")?.into()).await?;
            Ok(json!({ "changed": true, "filename": filename }))
        }
        "backup.export" => {
            service::backup::export(
                required_string(&request.params, "filename")?.into(),
                required_string(&request.params, "destination")?.into(),
            )
            .await?;
            Ok(json!({ "changed": true }))
        }
        "service.status" => {
            let available = cmd::is_service_available().await.is_ok();
            Ok(json!({ "available": available }))
        }
        "service.operate" => {
            let operation = required_string(&request.params, "operation")?;
            let result = match operation {
                "install" => cmd::install_service().await,
                "uninstall" => cmd::uninstall_service().await,
                "reinstall" => cmd::reinstall_service().await,
                "repair" => cmd::repair_service().await,
                _ => bail!("invalid service operation: {operation}"),
            };
            result.map_err(|error| anyhow::anyhow!(error))?;
            Ok(json!({ "changed": true, "operation": operation }))
        }
        "network.hostname" => Ok(json!(cmd::get_system_hostname())),
        "network.interfaces" => Ok(json!(cmd::get_network_interfaces())),
        "lightweight.status" => Ok(json!({
            "running_mode": CoreManager::global().get_running_mode().to_string(),
        })),
        "lightweight.set" => {
            let enabled = request
                .params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| anyhow::anyhow!("missing boolean parameter: enabled"))?;
            if enabled {
                cmd::entry_lightweight_mode().await
            } else {
                cmd::exit_lightweight_mode().await
            }
            .map_err(|error| anyhow::anyhow!(error))?;
            Ok(json!({ "changed": true, "enabled": enabled }))
        }
        "proxy.groups" => service::proxy::groups().await,
        "proxy.select" => {
            service::proxy::select(
                required_string(&request.params, "group")?,
                required_string(&request.params, "node")?,
            )
            .await?;
            Ok(json!({ "changed": true }))
        }
        "connection.list" => service::proxy::connections().await,
        "connection.close" => {
            service::proxy::close_connection(request.params.get("id").and_then(Value::as_str)).await?;
            Ok(json!({ "changed": true }))
        }
        "dns.show" => {
            let content = cmd::get_dns_config_content()
                .await
                .map_err(|error| anyhow::anyhow!(error))?;
            Ok(json!({ "content": content }))
        }
        "dns.apply" => {
            let enabled = request
                .params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| anyhow::anyhow!("missing boolean parameter: enabled"))?;
            cmd::apply_dns_config(enabled)
                .await
                .map_err(|error| anyhow::anyhow!(error))?;
            Ok(json!({ "changed": true, "enabled": enabled }))
        }
        "dns.validate" => {
            let outcome = cmd::validate_dns_config()
                .await
                .map_err(|error| anyhow::anyhow!(error))?;
            Ok(serde_json::to_value(outcome)?)
        }
        "webdav.configure" => {
            cmd::save_webdav_config(
                required_string(&request.params, "url")?.into(),
                required_string(&request.params, "username")?.into(),
                required_string(&request.params, "password")?.into(),
            )
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
            Ok(json!({ "changed": true }))
        }
        "webdav.backup" => {
            cmd::create_webdav_backup()
                .await
                .map_err(|error| anyhow::anyhow!(error))?;
            Ok(json!({ "changed": true }))
        }
        "webdav.list" => Ok(serde_json::to_value(
            cmd::list_webdav_backup()
                .await
                .map_err(|error| anyhow::anyhow!(error))?,
        )?),
        "webdav.delete" => {
            cmd::delete_webdav_backup(required_string(&request.params, "filename")?.into())
                .await
                .map_err(|error| anyhow::anyhow!(error))?;
            Ok(json!({ "changed": true }))
        }
        "webdav.restore" => {
            cmd::restore_webdav_backup(required_string(&request.params, "filename")?.into())
                .await
                .map_err(|error| anyhow::anyhow!(error))?;
            Ok(json!({ "changed": true }))
        }
        method => bail!("unknown CLI method: {method}"),
    }
}

fn required_string<'a>(params: &'a Value, name: &str) -> Result<&'a str> {
    params
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing string parameter: {name}"))
}

async fn handle_cli_request(
    authorization: Option<String>,
    request: CliRequest,
    token: Option<String>,
) -> std::result::Result<impl warp::Reply, warp::Rejection> {
    let response = if token.is_none() {
        CliResponse::failure(request.id, "unavailable", "CLI bridge initialization failed")
    } else if !authorized(authorization.as_deref(), token.as_deref().unwrap_or_default()) {
        CliResponse::failure(request.id, "authentication_failed", "invalid CLI token")
    } else {
        match dispatch_cli_request(&request).await {
            Ok(data) => CliResponse::success(request.id, data),
            Err(error) => CliResponse::failure(request.id, "command_failed", error.to_string()),
        }
    };

    Ok(warp::reply::json(&response))
}

/// check whether there is already exists
pub async fn check_singleton() -> Result<()> {
    let port = IVerge::get_singleton_port();
    if is_port_in_use(port) {
        let client = ClientBuilder::new().timeout(Duration::from_millis(500)).build()?;
        // 需要确保 Send
        #[allow(clippy::needless_collect)]
        let argvs: Vec<std::string::String> = std::env::args().collect();
        if argvs.len() > 1 {
            #[cfg(not(target_os = "macos"))]
            {
                let param = argvs[1].as_str();
                if param.starts_with("clash:") {
                    client
                        .get(format!("http://127.0.0.1:{port}/commands/scheme?param={param}"))
                        .send()
                        .await?;
                }
            }
        } else {
            client
                .get(format!("http://127.0.0.1:{port}/commands/visible"))
                .send()
                .await?;
        }
        logging!(error, Type::Window, "failed to setup singleton listen server");
        bail!("app exists");
    }
    Ok(())
}

/// The embed server only be used to implement singleton process
/// maybe it can be used as pac server later
pub fn embed_server() {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    #[allow(clippy::expect_used)]
    SHUTDOWN_SENDER
        .set(Mutex::new(Some(shutdown_tx)))
        .expect("failed to set shutdown signal for embedded server");
    let port = IVerge::get_singleton_port();
    let cli_token = ensure_cli_token()
        .map_err(|error| {
            logging!(error, Type::Setup, "Failed to initialize CLI token: {error}");
            error
        })
        .ok();

    let visible = warp::path!("commands" / "visible").and_then(|| async {
        logging!(info, Type::Window, "检测到从单例模式恢复应用窗口");
        if !lightweight::exit_lightweight_mode().await {
            WindowManager::show_main_window().await;
        } else {
            logging!(error, Type::Window, "轻量模式退出失败，无法恢复应用窗口");
        };
        Ok::<_, warp::Rejection>(warp::reply::with_status::<std::string::String>(
            "ok".to_string(),
            warp::http::StatusCode::OK,
        ))
    });

    let pac = warp::path!("commands" / "pac").and_then(|| async move {
        let verge_config = Config::verge().await;
        let clash_config = Config::clash().await;

        let verge_data = verge_config.data_arc();
        let clash_data = clash_config.data_arc();

        let pac_content = verge_data.pac_file_content.as_deref().unwrap_or(DEFAULT_PAC);

        let pac_port = verge_data
            .verge_mixed_port
            .unwrap_or_else(|| clash_data.get_mixed_port());
        let processed_content = pac_content.replace("%mixed-port%", &format!("{pac_port}"));
        Ok::<_, warp::Rejection>(
            warp::http::Response::builder()
                .header("Content-Type", "application/x-ns-proxy-autoconfig")
                .body(processed_content)
                .unwrap_or_default(),
        )
    });

    // Use map instead of and_then to avoid Send issues
    let scheme = warp::path!("commands" / "scheme")
        .and(warp::query::<QueryParam>())
        .and_then(|query: QueryParam| async move {
            AsyncHandler::spawn(|| async move {
                logging_error!(Type::Setup, resolve::resolve_scheme(&query.param).await);
            });
            Ok::<_, warp::Rejection>(warp::reply::with_status::<std::string::String>(
                "ok".to_string(),
                warp::http::StatusCode::OK,
            ))
        });

    let cli = warp::path!("cli" / "v1" / "invoke")
        .and(warp::post())
        .and(warp::header::optional::<String>("authorization"))
        .and(warp::body::json::<CliRequest>())
        .and(warp::any().map(move || cli_token.clone()))
        .and_then(handle_cli_request);

    let commands = visible.or(scheme).or(pac).or(cli);

    AsyncHandler::spawn(move || async move {
        warp::serve(commands)
            .bind(([127, 0, 0, 1], port))
            .await
            .graceful(async {
                shutdown_rx.await.ok();
            })
            .run()
            .await;
    });
}

pub fn shutdown_embedded_server() {
    logging!(info, Type::Window, "shutting down embedded server");
    if let Some(sender) = SHUTDOWN_SENDER.get()
        && let Some(sender) = sender.lock().take()
    {
        sender.send(()).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_cli_token_has_256_bits() {
        let token = generate_cli_token().expect("generate token");
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn authorization_requires_exact_bearer_token() {
        assert!(authorized(Some("Bearer abc"), "abc"));
        assert!(!authorized(Some("Bearer abcd"), "abc"));
        assert!(!authorized(Some("abc"), "abc"));
        assert!(!authorized(None, "abc"));
    }
}
