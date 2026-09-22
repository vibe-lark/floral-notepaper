//! A single user-facing link; IDs, CLI location and schema checks stay native.
use super::{
    lark_sync::{self, error, run_cli, CliRemote, SyncSettings},
    notes::{AppError, NoteStore},
};
use serde::Serialize;
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use url::Url;

#[derive(Clone, Debug, Serialize)]
pub struct SyncConnection {
    pub url: String,
    pub enabled: bool,
}

pub fn connection(store: &NoteStore) -> Result<SyncConnection, AppError> {
    let settings = lark_sync::get_settings(store)?;
    Ok(SyncConnection {
        url: settings.base_url,
        enabled: settings.enabled,
    })
}

pub fn validate_link(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    let parsed =
        Url::parse(value).map_err(|_| error("syncLink", "请粘贴完整的飞书多维表格链接"))?;
    let host = parsed.host_str().unwrap_or_default();
    let trusted = ["feishu.cn", "larkoffice.com", "larksuite.com"]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")));
    let path: Vec<_> = parsed
        .path_segments()
        .map(|parts| parts.collect())
        .unwrap_or_default();
    if value.len() > 4096
        || value.chars().any(char::is_control)
        || parsed.scheme() != "https"
        || !trusted
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || path.len() != 2
        || !["base", "wiki"].contains(&path[0])
        || path[1].is_empty()
        || !path[1].bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(error("syncLink", "请使用飞书多维表格的 HTTPS 链接（/base/ 或 /wiki/），不要填写令牌、应用页或其他网站地址"));
    }
    Ok(value.into())
}

fn executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn find_cli(configured: &str) -> Result<PathBuf, AppError> {
    let mut paths = Vec::new();
    if Path::new(configured).is_absolute() {
        paths.push(PathBuf::from(configured));
    }
    let name = if cfg!(windows) {
        "lark-cli.exe"
    } else {
        "lark-cli"
    };
    if let Some(path) = env::var_os("PATH") {
        paths.extend(
            env::split_paths(&path)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(name)),
        );
    }
    if let Some(home) = dirs::home_dir() {
        for suffix in [".local/bin", ".npm-global/bin", "bin"] {
            paths.push(home.join(suffix).join(name));
        }
    }
    for directory in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        paths.push(Path::new(directory).join(name));
    }
    paths
        .into_iter()
        .find(|path| executable(path))
        .ok_or_else(|| {
            error("syncCliMissing",
        "尚未找到 Lark CLI。请先在这台电脑安装并登录 Lark CLI，然后重新连接；无需填写程序路径")
        })
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub fn resolved_coordinates(resolved: &Value) -> Result<(String, Option<String>), AppError> {
    let base = resolved["base_token"]
        .as_str()
        .filter(|s| valid_id(s))
        .ok_or_else(|| error("syncLink", "链接没有解析为可访问的多维表格，请检查分享权限"))?;
    if resolved["selection_source"] == "url_query" {
        if resolved["block_type"] != "table" {
            return Err(error(
                "syncLink",
                "链接选中的是仪表盘或其他内容，请打开便签数据表后重新复制链接",
            ));
        }
        let table = resolved["table_id"]
            .as_str()
            .filter(|s| valid_id(s) && s.starts_with("tbl"))
            .ok_or_else(|| {
                error(
                    "syncLink",
                    "无法从链接取得数据表，请打开便签数据表后重新复制链接",
                )
            })?;
        Ok((base.into(), Some(table.into())))
    } else {
        Ok((base.into(), None))
    }
}

pub fn only_table(data: &Value) -> Result<String, AppError> {
    let tables = data["tables"]
        .as_array()
        .ok_or_else(|| error("syncLink", "无法读取数据表列表"))?;
    if data["total"].as_u64() != Some(1) || tables.len() != 1 {
        return Err(error("syncLink", "这个链接下有多张表或没有数据表，请打开便签所在的数据表后复制完整地址；仍只需填写一个链接"));
    }
    tables[0]["id"]
        .as_str()
        .filter(|s| valid_id(s) && s.starts_with("tbl"))
        .map(str::to_owned)
        .ok_or_else(|| error("syncLink", "数据表标识无效"))
}

fn resolve_target(store: &NoteStore, link: &str) -> Result<SyncSettings, AppError> {
    let url = validate_link(link)?;
    let mut settings = lark_sync::get_settings(store)?;
    settings.cli_path = find_cli(&settings.cli_path)?.to_string_lossy().into_owned();
    fs::create_dir_all(store.data_dir())?;
    let resolved = run_cli(
        &settings,
        store.data_dir(),
        &[
            "base".into(),
            "+url-resolve".into(),
            "--url".into(),
            url.clone(),
        ],
    )?;
    let (base, table) = resolved_coordinates(&resolved)?;
    settings.base_token = base;
    settings.table_id = match table {
        Some(table) => table,
        None => only_table(&run_cli(
            &settings,
            store.data_dir(),
            &[
                "base".into(),
                "+table-list".into(),
                "--base-token".into(),
                settings.base_token.clone(),
                "--limit".into(),
                "2".into(),
            ],
        )?)?,
    };
    settings.base_url = url;
    settings.interval_seconds = 60;
    settings.enabled = true;
    settings.validate()?;
    Ok(settings)
}

/// Read-only resolution and schema check, shared by the headless verifier.
pub fn resolve_link(store: &NoteStore, link: &str) -> Result<SyncSettings, AppError> {
    let settings = resolve_target(store, link)?;
    CliRemote::new(settings.clone(), store.data_dir())?.check_schema()?;
    Ok(settings)
}

pub fn connect(store: &NoteStore, link: &str) -> Result<SyncConnection, AppError> {
    let settings = resolve_target(store, link)?;
    // Save checks the target and schema before replacing a working connection.
    lark_sync::save_settings(store, settings)?;
    connection(store)
}

pub fn disconnect(store: &NoteStore) -> Result<SyncConnection, AppError> {
    let mut settings = lark_sync::get_settings(store)?;
    settings.enabled = false;
    lark_sync::save_settings(store, settings)?;
    connection(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn accepts_base_and_wiki_links_but_rejects_foreign_hosts_and_credentials() {
        for link in [
            "https://example.feishu.cn/base/test?table=tblTest",
            "https://example.larkoffice.com/wiki/Test",
            "https://example.larksuite.com/base/test",
        ] {
            assert_eq!(validate_link(link).unwrap(), link);
        }
        for link in [
            "testToken",
            "http://example.feishu.cn/base/test",
            "https://feishu.cn.evil.test/base/test",
            "https://evil.test/base/test",
            "https://user:secret@example.feishu.cn/base/test",
            "file:///base/test",
            "https://example.feishu.cn:444/base/test",
            "https://example.feishu.cn/app/test",
            "https://example.feishu.cn/base/",
            "https://example.feishu.cn/base/test\n?table=tblTest",
        ] {
            assert!(validate_link(link).is_err(), "accepted {link}");
        }
    }
    #[test]
    fn uses_resolved_table_not_the_raw_query_or_default_first_table() {
        assert_eq!(resolved_coordinates(&json!({"base_token":"basTest", "selection_source":"url_query", "block_type":"table", "table_id":"tblTest"})).unwrap(), ("basTest".into(), Some("tblTest".into())));
        assert!(resolved_coordinates(&json!({"base_token":"basTest", "selection_source":"url_query", "block_type":"dashboard", "table_id":"tblTest"})).is_err());
        assert_eq!(resolved_coordinates(&json!({"base_token":"basTest", "selection_source":"default", "table_id":"tblFirst"})).unwrap().1, None);
        assert!(resolved_coordinates(&json!({"app_token":"appTest"})).is_err());
    }
    #[test]
    fn a_bare_base_link_requires_exactly_one_table() {
        assert_eq!(
            only_table(&json!({"total":1,"tables":[{"id":"tblOne"}]})).unwrap(),
            "tblOne"
        );
        for value in [
            json!({"total":2,"tables":[{"id":"tblOne"}]}),
            json!({"total":0,"tables":[]}),
            json!({"tables":[{"id":"tblOne"}]}),
        ] {
            assert!(only_table(&value).is_err());
        }
    }
    #[test]
    fn old_config_remains_readable_and_invalid_link_does_not_replace_it() {
        let root =
            std::env::temp_dir().join(format!("larknote-link-test-{}", uuid::Uuid::new_v4()));
        let store = NoteStore::new(root.join("config"), root.join("data"));
        let old = json!({"enabled":true,"baseToken":"basTest","tableId":"tblTest","profile":"custom","cliPath":"lark-cli","intervalSeconds":90});
        crate::json_io::write_json_atomic(&store.config_dir().join("lark-sync.json"), &old)
            .unwrap();
        let loaded = lark_sync::get_settings(&store).unwrap();
        assert_eq!(loaded.base_url, "");
        assert_eq!(loaded.profile, "custom");
        assert!(connection(&store).unwrap().enabled);
        assert!(connect(&store, "https://evil.test/base/test").is_err());
        assert_eq!(lark_sync::get_settings(&store).unwrap(), loaded);
        let paused = disconnect(&store).unwrap();
        assert!(!paused.enabled);
        let paused = lark_sync::get_settings(&store).unwrap();
        assert_eq!(paused.base_token, loaded.base_token);
        assert_eq!(paused.profile, "custom");
    }
    #[test]
    fn explicit_executable_is_detected_without_shell_or_path_changes() {
        let executable = std::env::current_exe().unwrap();
        assert_eq!(find_cli(executable.to_str().unwrap()).unwrap(), executable);
    }

    #[cfg(unix)]
    #[test]
    fn cli_auth_and_schema_failures_leave_existing_connection_unchanged() {
        use std::os::unix::fs::PermissionsExt;
        for (script, expected) in [
            ("#!/bin/sh\nprintf '%s\\n' '{\"ok\":false,\"error\":{\"code\":99991679}}' >&2\nexit 1\n", "syncApi"),
            ("#!/bin/sh\ncase \"$*\" in\n*+url-resolve*) printf '%s\\n' '{\"ok\":true,\"data\":{\"base_token\":\"basNew\",\"table_id\":\"tblNew\",\"selection_source\":\"url_query\",\"block_type\":\"table\"}}' ;;\n*) printf '%s\\n' '{\"ok\":true,\"data\":{\"fields\":[]}}' ;;\nesac\n", "syncSchema"),
        ] {
            let root = env::temp_dir().join(format!("larknote-link-failure-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&root).unwrap();
            let cli = root.join("mock-cli");
            fs::write(&cli, script).unwrap();
            fs::set_permissions(&cli, fs::Permissions::from_mode(0o700)).unwrap();
            let store = NoteStore::new(root.join("config"), root.join("data"));
            let settings = SyncSettings { enabled: true, base_token: "basOld".into(), table_id: "tblOld".into(),
                profile: "existing-account".into(), cli_path: cli.to_string_lossy().into_owned(), ..Default::default() };
            let path = store.config_dir().join("lark-sync.json");
            crate::json_io::write_json_atomic(&path, &settings).unwrap();
            let before = fs::read(&path).unwrap();
            assert_eq!(connect(&store, "https://example.feishu.cn/base/test?table=tblTest").unwrap_err().code, expected);
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }
}
