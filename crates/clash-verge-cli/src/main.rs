use clash_verge_cli_protocol::{API_PATH, CliRequest, CliResponse, CliResponseError, TOKEN_FILE};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, Read as _, Write as _},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(not(feature = "verge-dev"))]
const APP_ID: &str = "io.github.clash-verge-rev.clash-verge-rev";
#[cfg(feature = "verge-dev")]
const APP_ID: &str = "io.github.clash-verge-rev.clash-verge-rev.dev";
const CLASH_CONFIG: &str = "config.yaml";
const VERGE_CONFIG: &str = "verge.yaml";
const PROFILE_YAML: &str = "profiles.yaml";
#[cfg(not(feature = "verge-dev"))]
const SINGLETON_SERVER: u16 = 33331;
#[cfg(feature = "verge-dev")]
const SINGLETON_SERVER: u16 = 11233;

const HELP: &str = r"Clash Verge Rev CLI

Usage:
  clash-verge-cli [--json] <command> [args]

Commands:
  status                         Show app/config status
  app dir                        Print the app config directory
  core info                      Show core ports and controller config
  core restart                   Restart the running Mihomo core
  core mode <rule|global|direct> Change the running core mode
  setting get [key]              Show all Verge settings or one dotted key
  setting set <key> <value>      Patch a Verge setting through the GUI
  profile list                   List profiles
  profile switch <id-or-name>    Switch the active profile through the GUI
  help                           Show this help

Options:
  --json                         Print machine-readable JSON
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug)]
struct Cli {
    format: OutputFormat,
    command: Command,
}

#[derive(Debug)]
enum Command {
    Status,
    AppDir,
    CoreInfo,
    CoreRestart,
    CoreMode { mode: String },
    SettingGet { key: Option<String> },
    SettingSet { key: String, value: String },
    ProfileList,
    ProfileSwitch { profile: String },
    Help,
}

#[derive(Debug)]
struct AppPaths {
    home: PathBuf,
}

#[derive(Debug)]
struct ConfigSnapshot {
    app_home: PathBuf,
    verge: Value,
    clash: Value,
    profiles: Value,
    verge_exists: bool,
    clash_exists: bool,
    profiles_exists: bool,
}

#[derive(Debug)]
struct CliError {
    code: ExitCode,
    message: String,
}

#[derive(Debug, Clone, Copy)]
enum ExitCode {
    Success = 0,
    GenericError = 1,
    InvalidArgument = 2,
    Unavailable = 3,
    AuthenticationFailed = 5,
    NotFound = 9,
}

impl CliError {
    fn new(code: ExitCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

type CliResult<T> = std::result::Result<T, CliError>;

fn main() {
    std::process::exit(run_from_env());
}

fn run_from_env() -> i32 {
    let args = env::args().skip(1).collect::<Vec<_>>();
    run(args)
}

fn run(args: Vec<String>) -> i32 {
    match run_inner(args) {
        Ok(()) => ExitCode::Success as i32,
        Err(err) => {
            let _ = writeln!(io::stderr(), "error: {}", err.message);
            err.code as i32
        }
    }
}

fn run_inner(args: Vec<String>) -> CliResult<()> {
    let cli = parse_args(args)?;
    match cli.command {
        Command::Help => {
            print_human(HELP);
            Ok(())
        }
        Command::AppDir => {
            let paths = resolve_app_paths()?;
            emit(cli.format, json!({ "app_home": paths.home }), || {
                paths.home.display().to_string()
            })
        }
        Command::Status => match bridge_call("status", json!({})) {
            Ok(payload) => emit(cli.format, payload.clone(), || bridge_status_human(&payload)),
            Err(error) if matches!(error.code, ExitCode::Unavailable) => {
                let snapshot = load_snapshot()?;
                let payload = status_payload(&snapshot);
                emit(cli.format, payload, || status_human(&snapshot))
            }
            Err(error) => Err(error),
        },
        Command::CoreInfo => {
            let snapshot = load_snapshot()?;
            let payload = core_info_payload(&snapshot);
            emit(cli.format, payload, || core_info_human(&snapshot))
        }
        Command::CoreRestart => {
            let payload = bridge_call("core.restart", json!({}))?;
            emit(cli.format, payload, || "Core restarted".to_string())
        }
        Command::CoreMode { mode } => {
            let payload = bridge_call("core.mode", json!({ "mode": mode }))?;
            emit(cli.format, payload, || format!("Core mode changed to {mode}"))
        }
        Command::SettingGet { key } => {
            let snapshot = load_snapshot()?;
            let verge = redact_value(snapshot.verge);
            let payload = match key {
                Some(key) => get_dotted(&verge, &key)
                    .cloned()
                    .ok_or_else(|| CliError::new(ExitCode::NotFound, format!("setting key not found: {key}")))?,
                None => verge,
            };
            emit(cli.format, payload.clone(), || human_value(&payload))
        }
        Command::SettingSet { key, value } => {
            if key.contains('.') {
                return Err(CliError::new(
                    ExitCode::InvalidArgument,
                    "setting set currently accepts top-level Verge keys only",
                ));
            }
            let value = parse_cli_value(&value)?;
            let payload = bridge_call("verge.patch", json!({ key.clone(): value }))?;
            emit(cli.format, payload, || format!("Setting updated: {key}"))
        }
        Command::ProfileList => {
            let snapshot = load_snapshot()?;
            let payload = profile_list_payload(&snapshot);
            emit(cli.format, payload, || profile_list_human(&snapshot))
        }
        Command::ProfileSwitch { profile } => {
            let payload = bridge_call("profile.switch", json!({ "profile": profile }))?;
            emit(cli.format, payload, || format!("Profile switched: {profile}"))
        }
    }
}

fn parse_args(args: Vec<String>) -> CliResult<Cli> {
    let mut format = OutputFormat::Human;
    let mut rest = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--json" => format = OutputFormat::Json,
            "-h" | "--help" => rest.push("help".to_string()),
            _ => rest.push(arg),
        }
    }

    let command = match rest.as_slice() {
        [] => Command::Help,
        [cmd] if cmd == "help" => Command::Help,
        [cmd] if cmd == "status" => Command::Status,
        [cmd, sub] if cmd == "app" && sub == "dir" => Command::AppDir,
        [cmd, sub] if cmd == "core" && sub == "info" => Command::CoreInfo,
        [cmd, sub] if cmd == "core" && sub == "restart" => Command::CoreRestart,
        [cmd, sub, mode] if cmd == "core" && sub == "mode" && matches!(mode.as_str(), "rule" | "global" | "direct") => {
            Command::CoreMode { mode: mode.clone() }
        }
        [cmd, sub] if cmd == "setting" && sub == "get" => Command::SettingGet { key: None },
        [cmd, sub, key] if cmd == "setting" && sub == "get" => Command::SettingGet { key: Some(key.clone()) },
        [cmd, sub, key, value] if cmd == "setting" && sub == "set" => Command::SettingSet {
            key: key.clone(),
            value: value.clone(),
        },
        [cmd, sub] if cmd == "profile" && sub == "list" => Command::ProfileList,
        [cmd, sub, profile] if cmd == "profile" && sub == "switch" => Command::ProfileSwitch {
            profile: profile.clone(),
        },
        [cmd, ..] => {
            return Err(CliError::new(
                ExitCode::InvalidArgument,
                format!("unknown command: {cmd}"),
            ));
        }
    };

    Ok(Cli { format, command })
}

fn resolve_app_paths() -> CliResult<AppPaths> {
    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let portable_root = dir.join(".config");
        if portable_root.join("PORTABLE").exists() {
            return Ok(AppPaths {
                home: portable_root.join(APP_ID),
            });
        }
    }

    Ok(AppPaths {
        home: platform_data_dir()?.join(APP_ID),
    })
}

#[cfg(target_os = "windows")]
fn platform_data_dir() -> CliResult<PathBuf> {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::new(ExitCode::Unavailable, "APPDATA is not set"))
}

#[cfg(target_os = "macos")]
fn platform_data_dir() -> CliResult<PathBuf> {
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library").join("Application Support"))
        .ok_or_else(|| CliError::new(ExitCode::Unavailable, "HOME is not set"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_data_dir() -> CliResult<PathBuf> {
    if let Some(dir) = env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(dir));
    }

    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".local").join("share"))
        .ok_or_else(|| CliError::new(ExitCode::Unavailable, "HOME is not set"))
}

fn load_snapshot() -> CliResult<ConfigSnapshot> {
    let paths = resolve_app_paths()?;
    let verge_path = paths.home.join(VERGE_CONFIG);
    let clash_path = paths.home.join(CLASH_CONFIG);
    let profiles_path = paths.home.join(PROFILE_YAML);

    let (verge, verge_exists) = read_yaml_json(&verge_path)?;
    let (clash, clash_exists) = read_yaml_json(&clash_path)?;
    let (profiles, profiles_exists) = read_yaml_json(&profiles_path)?;

    Ok(ConfigSnapshot {
        app_home: paths.home,
        verge,
        clash,
        profiles,
        verge_exists,
        clash_exists,
        profiles_exists,
    })
}

fn read_yaml_json(path: &PathBuf) -> CliResult<(Value, bool)> {
    if !path.exists() {
        return Ok((json!({}), false));
    }

    let content = fs::read_to_string(path).map_err(|err| {
        CliError::new(
            ExitCode::GenericError,
            format!("failed to read {}: {err}", path.display()),
        )
    })?;

    let yaml = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&content).map_err(|err| {
        CliError::new(
            ExitCode::InvalidArgument,
            format!("failed to parse {}: {err}", path.display()),
        )
    })?;

    let json = serde_json::to_value(yaml).map_err(|err| {
        CliError::new(
            ExitCode::InvalidArgument,
            format!("failed to convert {} to JSON: {err}", path.display()),
        )
    })?;

    Ok((json, true))
}

fn parse_cli_value(value: &str) -> CliResult<Value> {
    let yaml = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(value)
        .map_err(|error| CliError::new(ExitCode::InvalidArgument, format!("invalid setting value: {error}")))?;
    serde_json::to_value(yaml).map_err(|error| CliError::new(ExitCode::InvalidArgument, error.to_string()))
}

fn bridge_call(method: &str, params: Value) -> CliResult<Value> {
    let paths = resolve_app_paths()?;
    let token = fs::read_to_string(paths.home.join(TOKEN_FILE)).map_err(|_| {
        CliError::new(
            ExitCode::Unavailable,
            "GUI CLI bridge is not available; start Clash Verge first",
        )
    })?;
    let token = token.trim();
    if token.is_empty() {
        return Err(CliError::new(ExitCode::Unavailable, "GUI CLI bridge token is empty"));
    }

    let request = CliRequest {
        id: request_id(),
        method: method.to_string(),
        params,
    };
    let body =
        serde_json::to_vec(&request).map_err(|error| CliError::new(ExitCode::GenericError, error.to_string()))?;
    let addr = SocketAddr::from(([127, 0, 0, 1], SINGLETON_SERVER));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).map_err(|_| {
        CliError::new(
            ExitCode::Unavailable,
            "GUI CLI bridge is not reachable; start Clash Verge first",
        )
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| CliError::new(ExitCode::GenericError, error.to_string()))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| CliError::new(ExitCode::GenericError, error.to_string()))?;

    let headers = format!(
        "POST {API_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{SINGLETON_SERVER}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .and_then(|_| stream.write_all(&body))
        .map_err(|error| CliError::new(ExitCode::Unavailable, error.to_string()))?;

    let mut response_bytes = Vec::new();
    stream
        .read_to_end(&mut response_bytes)
        .map_err(|error| CliError::new(ExitCode::Unavailable, error.to_string()))?;
    let response_body = extract_http_body(&response_bytes)?;
    let response = serde_json::from_slice::<CliResponse>(&response_body)
        .map_err(|error| CliError::new(ExitCode::GenericError, format!("invalid bridge response: {error}")))?;

    if response.ok {
        Ok(response.data.unwrap_or(Value::Null))
    } else {
        let error = response.error.unwrap_or_else(|| CliResponseError {
            code: "command_failed".to_string(),
            message: "CLI bridge command failed".to_string(),
        });
        Err(CliError::new(map_bridge_exit_code(&error.code), error.message))
    }
}

fn request_id() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string()
}

fn extract_http_body(response: &[u8]) -> CliResult<Vec<u8>> {
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| CliError::new(ExitCode::GenericError, "invalid HTTP response"))?;
    let headers = String::from_utf8_lossy(&response[..split]).to_ascii_lowercase();
    let body = &response[split + 4..];
    if headers.contains("transfer-encoding: chunked") {
        decode_chunked(body)
    } else {
        Ok(body.to_vec())
    }
}

fn decode_chunked(mut body: &[u8]) -> CliResult<Vec<u8>> {
    let mut output = Vec::new();
    loop {
        let line_end = body
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(|| CliError::new(ExitCode::GenericError, "invalid chunked response"))?;
        let size_text = std::str::from_utf8(&body[..line_end])
            .map_err(|error| CliError::new(ExitCode::GenericError, error.to_string()))?;
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or_default(), 16)
            .map_err(|error| CliError::new(ExitCode::GenericError, error.to_string()))?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Ok(output);
        }
        if body.len() < size + 2 {
            return Err(CliError::new(ExitCode::GenericError, "truncated chunked response"));
        }
        output.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

fn map_bridge_exit_code(code: &str) -> ExitCode {
    match code {
        "authentication_failed" => ExitCode::AuthenticationFailed,
        "unavailable" => ExitCode::Unavailable,
        "invalid_argument" => ExitCode::InvalidArgument,
        "not_found" => ExitCode::NotFound,
        _ => ExitCode::GenericError,
    }
}

fn emit<F>(format: OutputFormat, payload: Value, human: F) -> CliResult<()>
where
    F: FnOnce() -> String,
{
    match format {
        OutputFormat::Human => {
            print_human(&human());
            Ok(())
        }
        OutputFormat::Json => {
            let text = serde_json::to_string_pretty(&payload)
                .map_err(|err| CliError::new(ExitCode::GenericError, err.to_string()))?;
            print_human(&text);
            Ok(())
        }
    }
}

fn print_human(text: &str) {
    println!("{text}");
}

fn status_payload(snapshot: &ConfigSnapshot) -> Value {
    let current = current_profile(snapshot);

    json!({
        "app": {
            "running": is_gui_running(),
            "home": snapshot.app_home,
        },
        "config": {
            "verge": snapshot.verge_exists,
            "clash": snapshot.clash_exists,
            "profiles": snapshot.profiles_exists,
        },
        "core": {
            "mode": get_string(&snapshot.clash, "mode").unwrap_or("rule"),
            "mixed_port": get_u64(&snapshot.clash, "mixed-port").unwrap_or(7897),
            "external_controller": get_string(&snapshot.clash, "external-controller").unwrap_or("127.0.0.1:9097"),
        },
        "verge": {
            "system_proxy": get_bool(&snapshot.verge, "enable_system_proxy").unwrap_or(false),
            "tun": get_bool(&snapshot.verge, "enable_tun_mode").unwrap_or(false),
            "external_controller_enabled": get_bool(&snapshot.verge, "enable_external_controller").unwrap_or(false),
        },
        "profile": current,
    })
}

fn status_human(snapshot: &ConfigSnapshot) -> String {
    let current = current_profile(snapshot);
    let profile_name = current.get("name").and_then(Value::as_str).unwrap_or("<none>");
    let running = if is_gui_running() { "running" } else { "not detected" };
    let system_proxy = enabled_label(get_bool(&snapshot.verge, "enable_system_proxy").unwrap_or(false));
    let tun = enabled_label(get_bool(&snapshot.verge, "enable_tun_mode").unwrap_or(false));
    let mode = get_string(&snapshot.clash, "mode").unwrap_or("rule");

    format!(
        "App: {running}\nCore Mode: {mode}\nSystem Proxy: {system_proxy}\nTUN: {tun}\nProfile: {profile_name}\nConfig Dir: {}",
        snapshot.app_home.display()
    )
}

fn bridge_status_human(status: &Value) -> String {
    let running_mode = status
        .pointer("/core/running_mode")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let system_proxy = status
        .pointer("/verge/system_proxy")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let tun = status.pointer("/verge/tun").and_then(Value::as_bool).unwrap_or(false);
    let profile = status
        .pointer("/profile/name")
        .and_then(Value::as_str)
        .unwrap_or("<none>");

    format!(
        "App: running\nCore: {running_mode}\nSystem Proxy: {}\nTUN: {}\nProfile: {profile}",
        enabled_label(system_proxy),
        enabled_label(tun)
    )
}

fn core_info_payload(snapshot: &ConfigSnapshot) -> Value {
    json!({
        "mixed_port": get_u64(&snapshot.clash, "mixed-port").unwrap_or(7897),
        "socks_port": get_u64(&snapshot.clash, "socks-port").unwrap_or(7898),
        "http_port": get_u64(&snapshot.clash, "port").unwrap_or(7899),
        "mode": get_string(&snapshot.clash, "mode").unwrap_or("rule"),
        "external_controller": get_string(&snapshot.clash, "external-controller").unwrap_or("127.0.0.1:9097"),
        "external_controller_enabled": get_bool(&snapshot.verge, "enable_external_controller").unwrap_or(false),
    })
}

fn core_info_human(snapshot: &ConfigSnapshot) -> String {
    format!(
        "Mixed Port: {}\nSocks Port: {}\nHTTP Port: {}\nMode: {}\nExternal Controller: {}\nExternal Controller Enabled: {}",
        get_u64(&snapshot.clash, "mixed-port").unwrap_or(7897),
        get_u64(&snapshot.clash, "socks-port").unwrap_or(7898),
        get_u64(&snapshot.clash, "port").unwrap_or(7899),
        get_string(&snapshot.clash, "mode").unwrap_or("rule"),
        get_string(&snapshot.clash, "external-controller").unwrap_or("127.0.0.1:9097"),
        enabled_label(get_bool(&snapshot.verge, "enable_external_controller").unwrap_or(false)),
    )
}

fn profile_list_payload(snapshot: &ConfigSnapshot) -> Value {
    let current = snapshot.profiles.get("current").and_then(Value::as_str);
    let items = snapshot
        .profiles
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let uid = item.get("uid").and_then(Value::as_str).unwrap_or("");
                    json!({
                        "uid": uid,
                        "name": item.get("name").and_then(Value::as_str).unwrap_or(""),
                        "type": item.get("type").or_else(|| item.get("itype")).and_then(Value::as_str).unwrap_or(""),
                        "current": current == Some(uid),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    json!({ "current": current, "items": items })
}

fn profile_list_human(snapshot: &ConfigSnapshot) -> String {
    let current = snapshot.profiles.get("current").and_then(Value::as_str);
    let Some(items) = snapshot.profiles.get("items").and_then(Value::as_array) else {
        return "No profiles found".to_string();
    };

    if items.is_empty() {
        return "No profiles found".to_string();
    }

    items
        .iter()
        .map(|item| {
            let uid = item.get("uid").and_then(Value::as_str).unwrap_or("");
            let name = item.get("name").and_then(Value::as_str).unwrap_or("<unnamed>");
            let ty = item
                .get("type")
                .or_else(|| item.get("itype"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let marker = if current == Some(uid) { "*" } else { " " };
            format!("{marker} {name} ({ty}) [{uid}]")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn current_profile(snapshot: &ConfigSnapshot) -> Value {
    let current = snapshot.profiles.get("current").and_then(Value::as_str);
    let item = snapshot
        .profiles
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("uid").and_then(Value::as_str) == current)
        });

    match item {
        Some(item) => json!({
            "uid": current,
            "name": item.get("name").and_then(Value::as_str),
        }),
        None => json!({
            "uid": current,
            "name": null,
        }),
    }
}

fn is_gui_running() -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], SINGLETON_SERVER));
    TcpStream::connect_timeout(&addr, Duration::from_millis(120)).is_ok()
}

fn get_dotted<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.').try_fold(value, |current, part| current.get(part))
}

fn get_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn get_u64(value: &Value, key: &str) -> Option<u64> {
    match value.get(key) {
        Some(Value::Number(number)) => number.as_u64(),
        Some(Value::String(text)) => text.parse().ok(),
        _ => None,
    }
}

fn get_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

const fn enabled_label(value: bool) -> &'static str {
    if value { "enabled" } else { "disabled" }
}

fn human_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Null => "null".to_string(),
        _ => serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
    }
}

fn redact_value(mut value: Value) -> Value {
    redact_value_inner(&mut value);
    value
}

fn redact_value_inner(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let key = key.to_ascii_lowercase();
                if key.contains("password") || key.contains("secret") || key.contains("token") {
                    *value = Value::String("<redacted>".to_string());
                } else {
                    redact_value_inner(value);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_value_inner(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_global_json_before_command() {
        let cli = parse_args(vec!["--json".into(), "status".into()]).expect("parse");
        assert_eq!(cli.format, OutputFormat::Json);
        assert!(matches!(cli.command, Command::Status));
    }

    #[test]
    fn parse_setting_key() {
        let cli = parse_args(vec!["setting".into(), "get".into(), "enable_system_proxy".into()]).expect("parse");
        assert!(matches!(
            cli.command,
            Command::SettingGet { key: Some(ref key) } if key == "enable_system_proxy"
        ));
    }

    #[test]
    fn parse_mutating_commands() {
        let mode = parse_args(vec!["core".into(), "mode".into(), "global".into()]).expect("parse mode");
        assert!(matches!(
            mode.command,
            Command::CoreMode { ref mode } if mode == "global"
        ));

        let setting = parse_args(vec![
            "setting".into(),
            "set".into(),
            "enable_system_proxy".into(),
            "true".into(),
        ])
        .expect("parse setting");
        assert!(matches!(
            setting.command,
            Command::SettingSet { ref key, ref value }
                if key == "enable_system_proxy" && value == "true"
        ));
    }

    #[test]
    fn parses_cli_values_as_yaml_scalars() {
        assert_eq!(parse_cli_value("true").expect("bool"), json!(true));
        assert_eq!(parse_cli_value("7897").expect("number"), json!(7897));
        assert_eq!(parse_cli_value("system").expect("string"), json!("system"));
    }

    #[test]
    fn decodes_chunked_http_body() {
        let response = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n4\r\n{\"ok\r\n4\r\n\":1}\r\n0\r\n\r\n";
        assert_eq!(extract_http_body(response).expect("decode"), br#"{"ok":1}"#);
    }

    #[test]
    fn dotted_lookup_reads_nested_values() {
        let value = json!({ "a": { "b": true } });
        assert_eq!(get_dotted(&value, "a.b").and_then(Value::as_bool), Some(true));
        assert!(get_dotted(&value, "a.c").is_none());
    }

    #[test]
    fn redacts_sensitive_fields() {
        let value = redact_value(json!({
            "secret": "abc",
            "webdav_password": "pass",
            "nested": { "token": "tok", "normal": "ok" }
        }));
        assert_eq!(value["secret"], "<redacted>");
        assert_eq!(value["webdav_password"], "<redacted>");
        assert_eq!(value["nested"]["token"], "<redacted>");
        assert_eq!(value["nested"]["normal"], "ok");
    }
}
