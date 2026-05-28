#![allow(dead_code)]

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use tauri::Manager;

use std::io::Write;

use futures::stream::{self, StreamExt};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{fs, path::PathBuf, process::Command};
use tauri::Emitter;
use tauri::{command, AppHandle, State};
use walkdir::WalkDir;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use crate::core::instance_manager::Instance;
use crate::discord;
use crate::ilog;
use crate::ilog_err;
use crate::java_runtime::ensure_java;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct VersionJson {
    downloads: Downloads,
    libraries: Vec<Library>,
    #[serde(rename = "assetIndex")]
    asset_index: AssetIndex,
    #[serde(rename = "javaVersion")]
    java_version: Option<JavaVersion>,
}

#[derive(Debug, Deserialize)]
struct JavaVersion {
    #[serde(rename = "majorVersion")]
    major_version: u32,
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    id: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct Downloads {
    client: DownloadFile,
}

#[derive(Debug, Deserialize)]
struct DownloadFile {
    url: String,
}

#[derive(Debug, Deserialize)]
struct Library {
    name: Option<String>,
    downloads: Option<LibDownloads>,
    rules: Option<Vec<Rule>>,
}

#[derive(Debug, Deserialize)]
struct Rule {
    action: String,
    os: Option<OsRule>,
}

#[derive(Debug, Deserialize)]
struct OsRule {
    name: Option<String>,
    arch: Option<String>,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LibDownloads {
    artifact: Option<Artifact>,
    classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Deserialize)]
struct Artifact {
    url: String,
    path: String,
}

#[derive(Debug, Deserialize)]
struct ForgeVersionJson {
    #[serde(rename = "mainClass")]
    main_class: String,
    arguments: Option<ForgeArguments>,
}

#[derive(Debug, Deserialize)]
struct ForgeArguments {
    game: Option<Vec<Value>>,
    jvm: Option<Vec<Value>>,
}

fn current_os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

fn arch_matches(rule_arch: &str) -> bool {
    let arch = rule_arch.to_ascii_lowercase();
    if arch == "x86" {
        cfg!(target_arch = "x86")
    } else if arch == "x86_64" || arch == "amd64" {
        cfg!(target_arch = "x86_64")
    } else if arch == "arm64" || arch == "aarch64" || arch == "aarch_64" {
        cfg!(target_arch = "aarch64")
    } else {
        true
    }
}

fn os_rule_matches_parts(name: Option<&str>, arch: Option<&str>) -> bool {
    if let Some(name) = name {
        if name != current_os_name() {
            return false;
        }
    }
    if let Some(arch) = arch {
        if !arch_matches(arch) {
            return false;
        }
    }
    true
}

fn os_rule_matches(os: &OsRule) -> bool {
    let _ = &os.version;
    os_rule_matches_parts(os.name.as_deref(), os.arch.as_deref())
}

fn is_library_allowed(rules: &[Rule]) -> bool {
    let mut allowed = false;
    for rule in rules {
        let os_matches = match &rule.os {
            None => true,
            Some(os) => os_rule_matches(os),
        };
        if os_matches {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn is_native_library_name(name: &str) -> bool {
    name.split(':')
        .nth(3)
        .map(|classifier| classifier.to_ascii_lowercase().contains("natives-"))
        .unwrap_or(false)
}

fn native_classifier_matches_current_arch(name: &str) -> bool {
    let Some(classifier) = name.split(':').nth(3) else {
        return true;
    };
    let classifier = classifier.to_ascii_lowercase();
    if !classifier.contains("natives-") {
        return true;
    }
    if classifier.contains("arm64")
        || classifier.contains("aarch64")
        || classifier.contains("aarch_64")
    {
        return cfg!(target_arch = "aarch64");
    }
    if classifier.ends_with("-x86") || classifier.contains("x86_32") {
        return cfg!(target_arch = "x86");
    }
    true
}

fn should_use_manifest_library(lib: &Library) -> bool {
    if let Some(rules) = &lib.rules {
        if !is_library_allowed(rules) {
            return false;
        }
    }
    lib.name
        .as_deref()
        .map(native_classifier_matches_current_arch)
        .unwrap_or(true)
}

fn is_value_library_allowed(lib: &Value) -> bool {
    if let Some(rules) = lib.get("rules").and_then(|v| v.as_array()) {
        let mut allowed = false;
        for rule in rules {
            let os_matches = match rule.get("os") {
                None => true,
                Some(os) => os_rule_matches_parts(
                    os.get("name").and_then(|v| v.as_str()),
                    os.get("arch").and_then(|v| v.as_str()),
                ),
            };
            if os_matches {
                allowed = rule.get("action").and_then(|v| v.as_str()) == Some("allow");
            }
        }
        if !allowed {
            return false;
        }
    }
    lib.get("name")
        .and_then(|v| v.as_str())
        .map(native_classifier_matches_current_arch)
        .unwrap_or(true)
}

fn classpath_separator() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

fn artifact_path(root: &PathBuf, artifact: &str) -> PathBuf {
    let mut path = root.clone();
    for part in artifact.split('/') {
        if !part.is_empty() {
            path.push(part);
        }
    }
    path
}

fn maven_artifact_path_from_name(name: &str) -> Option<String> {
    let (coords, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied().unwrap_or("");
    let classifier_suffix = if classifier.is_empty() {
        String::new()
    } else {
        format!("-{}", classifier)
    };
    Some(format!(
        "{}/{}/{}/{}-{}{}.{}",
        group, artifact, version, artifact, version, classifier_suffix, ext
    ))
}

fn library_ga(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.splitn(3, ':').collect();
    if parts.len() >= 2 {
        Some(format!("{}:{}", parts[0], parts[1]))
    } else {
        None
    }
}

fn manifest_library_artifact_path(lib: &Library) -> Option<String> {
    lib.downloads
        .as_ref()
        .and_then(|d| d.artifact.as_ref())
        .map(|a| a.path.clone())
        .or_else(|| lib.name.as_deref().and_then(maven_artifact_path_from_name))
}

fn value_library_artifact_path(lib: &Value) -> Option<String> {
    lib.get("downloads")
        .and_then(|v| v.get("artifact"))
        .and_then(|v| v.get("path"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            lib.get("name")
                .and_then(|v| v.as_str())
                .and_then(maven_artifact_path_from_name)
        })
}

fn value_library_download_url(lib: &Value, artifact: &str) -> Option<String> {
    lib.get("downloads")
        .and_then(|v| v.get("artifact"))
        .and_then(|v| v.get("url"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            lib.get("url")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(|base| format!("{}/{}", base.trim_end_matches('/'), artifact))
        })
}

fn push_classpath_path(paths: &mut Vec<String>, seen: &mut HashSet<String>, path: PathBuf) {
    let path_str = path.to_string_lossy().to_string();
    let key = if cfg!(windows) {
        path_str.to_ascii_lowercase()
    } else {
        path_str.clone()
    };
    if seen.insert(key) {
        paths.push(path_str);
    }
}

#[command]
pub async fn get_instances(launcher_id: String) -> Result<Vec<Value>, String> {
    let api_url = "https://fitzxel-cl-api.vercel.app/v2";
    let url = format!("{}/{}/instances", api_url, launcher_id);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<Vec<Value>>()
        .await
        .map_err(|e| e.to_string())?;
    Ok(resp)
}

#[command]
pub async fn get_instance(
    launcher_id: String,
    id: Option<String>,
    slug: Option<String>,
    code: Option<String>,
) -> Result<Value, String> {
    let api_url = "https://fitzxel-cl-api.vercel.app/v2";
    let query = if let Some(i) = id {
        format!("id={}", i)
    } else if let Some(s) = slug {
        format!("slug={}", s)
    } else if let Some(c) = code {
        format!("code={}", urlencoding::encode(&c))
    } else {
        return Err("No instance specified".into());
    };
    let url = format!("{}/{}/instance?{}", api_url, launcher_id, query);
    let client = reqwest::Client::new();
    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if response.status() == 404 {
        return Err("404: Instance not found".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "Error {}: failed to get instance",
            response.status()
        ));
    }
    let data = response.json::<Value>().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[command]
pub fn discord_set_idle() {
    discord::set_idle();
}

#[command]
pub fn discord_set_playing(name: String) {
    discord::set_playing(&name);
}

#[command]
pub fn uninstall_instance(instance_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let instance_path = {
        let manager = state.instances.lock().unwrap();
        manager
            .instances
            .iter()
            .find(|i| i.id == instance_id)
            .map(|i| i.path.clone())
    };
    if let Some(path) = instance_path {
        if path.exists() {
            fs::remove_dir_all(&path)
                .map_err(|e| format!("Error removing instance folder: {}", e))?;
        }
    }
    {
        let mut manager = state.instances.lock().unwrap();
        manager.instances.retain(|i| i.id != instance_id);
    }
    Ok(())
}

#[command]
pub async fn install_instance_files(
    app: AppHandle,
    instance_id: String,
    instance_code: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let instance = {
        let manager = state.instances.lock().unwrap();
        manager
            .instances
            .iter()
            .find(|i| i.id == instance_id)
            .cloned()
            .ok_or_else(|| format!("Instance '{}' not found", instance_id))?
    };
    let log_id = format!("{}-{}", instance.id, instance.slug.as_deref().unwrap_or(""));
    let mods_dir = instance.path.join("mods");
    let mods_exist = mods_dir.exists()
        && fs::read_dir(&mods_dir)
            .map(|mut d| d.next().is_some())
            .unwrap_or(false);
    if instance.files_installed == Some(true) && mods_exist {
        ilog!(
            &app,
            &log_id,
            "Files already installed for '{}', skipping download",
            instance_id
        );
        app.emit("install-done", &instance_id).ok();
        return Ok(());
    }
    if instance.files_installed == Some(true) && !mods_exist {
        let mut manager = state.instances.lock().unwrap();
        manager.mark_files_installed(&instance_id);
        drop(manager);
    }
    let instance_dir = instance.path.clone();
    fs::create_dir_all(&instance_dir).ok();
    let api_url = "https://fitzxel-cl-api.vercel.app/v2".to_string();
    let url = if let Some(code) = &instance_code {
        format!("{}/instance/{}/files?code={}", api_url, instance_id, code)
    } else {
        format!("{}/instance/{}/files", api_url, instance_id)
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap_or_default();
    let files: Vec<Value> = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Error fetching files: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Error parsing files JSON: {}", e))?;
    ilog!(&app, &log_id, "Total remote files: {}", files.len());
    let sorted_files = files.clone();
    let overrides = sorted_files
        .iter()
        .find(|f| f["path"].as_str() == Some("overrides.zip"))
        .cloned();
    let regular_files: Vec<Value> = sorted_files
        .iter()
        .filter(|f| f["path"].as_str() != Some("overrides.zip"))
        .cloned()
        .collect();
    let total = sorted_files.len();
    let count = AtomicUsize::new(0);
    stream::iter(regular_files)
        .for_each_concurrent(256, |file| {
            let client = client.clone();
            let instance_dir = instance_dir.clone();
            let app = app.clone();
            let log_id = log_id.clone();
            let count = &count;
            async move {
                let file_path = file["path"].as_str().unwrap_or("").to_string();
                let downloads = file["downloads"]
                    .as_array()
                    .and_then(|d| d.first().cloned());
                if file_path.is_empty() {
                    count.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                let local_path = instance_dir.join(&file_path);
                if let Some(parent) = local_path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                if local_path.exists() {
                    let c = count.fetch_add(1, Ordering::Relaxed) + 1;
                    app.emit("install-progress", format!("{}/{}", c, total))
                        .ok();
                    return;
                }
                if let Some(dl_url) = downloads.and_then(|u| u.as_str().map(|s| s.to_string())) {
                    match client.get(&dl_url).send().await {
                        Ok(resp) => match resp.bytes().await {
                            Ok(bytes) => {
                                fs::write(&local_path, &bytes).ok();
                            }
                            Err(e) => {
                                ilog_err!(&app, &log_id, "Error reading bytes {}: {}", file_path, e)
                            }
                        },
                        Err(e) => {
                            ilog_err!(&app, &log_id, "Error downloading {}: {}", file_path, e)
                        }
                    }
                }
                let c = count.fetch_add(1, Ordering::Relaxed) + 1;
                app.emit("install-progress", format!("{}/{}", c, total))
                    .ok();
            }
        })
        .await;
    if let Some(file) = overrides {
        let file_path = "overrides.zip";
        let local_path = instance_dir.join(file_path);
        let downloads = file["downloads"].as_array();
        if let Some(dl_list) = downloads {
            if let Some(dl_url) = dl_list.first().and_then(|u| u.as_str()) {
                match client.get(dl_url).send().await {
                    Ok(resp) => match resp.bytes().await {
                        Ok(bytes) => {
                            fs::write(&local_path, &bytes).ok();
                        }
                        Err(e) => {
                            ilog_err!(&app, &log_id, "Error reading bytes overrides.zip: {}", e)
                        }
                    },
                    Err(e) => ilog_err!(&app, &log_id, "Error downloading overrides.zip: {}", e),
                }
            }
        }
        let c = count.fetch_add(1, Ordering::Relaxed) + 1;
        app.emit("install-progress", format!("{}/{}", c, total))
            .ok();
        if local_path.exists() {
            ilog!(&app, &log_id, "Extracting overrides.zip...");
            app.emit("install-status", "Extracting overrides...").ok();
            match fs::read(&local_path) {
                Ok(zip_bytes) => {
                    let cursor = std::io::Cursor::new(zip_bytes);
                    match zip::ZipArchive::new(cursor) {
                        Ok(mut archive) => {
                            for i in 0..archive.len() {
                                if let Ok(mut zip_file) = archive.by_index(i) {
                                    let out_path = instance_dir.join(zip_file.name());
                                    if zip_file.name().ends_with('/') {
                                        fs::create_dir_all(&out_path).ok();
                                    } else {
                                        if let Some(parent) = out_path.parent() {
                                            fs::create_dir_all(parent).ok();
                                        }
                                        if let Ok(mut out_file) = fs::File::create(&out_path) {
                                            std::io::copy(&mut zip_file, &mut out_file).ok();
                                        }
                                    }
                                }
                            }
                            ilog!(&app, &log_id, "overrides.zip extracted successfully");
                            fs::remove_file(&local_path).ok();
                        }
                        Err(e) => ilog_err!(&app, &log_id, "Error opening overrides.zip: {}", e),
                    }
                }
                Err(e) => ilog_err!(&app, &log_id, "Error reading overrides.zip: {}", e),
            }
        }
    }
    {
        let mut manager = state.instances.lock().unwrap();
        manager.mark_files_installed(&instance_id);
    }
    ilog!(
        &app,
        &log_id,
        "Installation complete: {}/{} files",
        total,
        total
    );
    app.emit("install-done", instance_id).ok();
    Ok(())
}

async fn download(url: &str, path: &PathBuf) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let bytes = reqwest::get(url)
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    fs::write(path, &bytes).map_err(|e| e.to_string())?;
    Ok(())
}

fn offline_uuid(username: &str) -> String {
    let name = format!("OfflinePlayer:{}", username);
    let digest = md5::compute(name.as_bytes());
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

async fn extract_natives(native_jar: &PathBuf, natives_dir: &PathBuf) -> Result<(), String> {
    let bytes = fs::read(native_jar).map_err(|e| e.to_string())?;
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        if let Ok(mut file) = archive.by_index(i) {
            let name = file.name().to_string();
            if name.starts_with("META-INF") {
                continue;
            }
            if name.ends_with('/') {
                continue;
            }
            let out = natives_dir.join(&name);
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).ok();
            }
            if let Ok(mut out_file) = fs::File::create(&out) {
                std::io::copy(&mut file, &mut out_file).ok();
            }
        }
    }
    Ok(())
}

async fn download_libraries(
    libraries: &[Library],
    cache_libs: &PathBuf,
    natives_dir: &PathBuf,
    app: &AppHandle,
    log_id: &str,
) -> Result<(), String> {
    let filtered: Vec<&Library> = libraries
        .iter()
        .filter(|lib| should_use_manifest_library(lib))
        .collect();

    let total = filtered.len();
    let mut current = 0usize;

    for lib in filtered {
        current += 1;
        app.emit("install-progress", format!("{}/{}", current, total)).ok();

        if let Some(d) = &lib.downloads {
            if let Some(a) = &d.artifact {
                let lib_path = cache_libs.join(&a.path);
                if let Err(e) = download(&a.url, &lib_path).await {
                    ilog!(app, log_id, "Warn downloading lib {}: {}", a.path, e);
                }
                if lib_path.exists()
                    && lib.name.as_deref().map(is_native_library_name).unwrap_or(false)
                {
                    if let Err(e) = extract_natives(&lib_path, natives_dir).await {
                        ilog!(app, log_id, "Warn extracting native {}: {}", a.path, e);
                    } else {
                        ilog!(app, log_id, "Native extracted: {}", a.path);
                    }
                }
            } else if let Some(name) = &lib.name {
                if let Some(maven_path) = maven_artifact_path_from_name(name) {
                    let lib_path = cache_libs.join(&maven_path);
                    if !lib_path.exists() {
                        let url = format!("https://libraries.minecraft.net/{}", maven_path);
                        ilog!(app, log_id, "Fallback download lib {}: {}", name, url);
                        if let Err(_) = download(&url, &lib_path).await {
                            let url2 = format!("https://repo1.maven.org/maven2/{}", maven_path);
                            if let Err(e) = download(&url2, &lib_path).await {
                                ilog!(app, log_id, "Warn fallback lib {}: {}", name, e);
                            }
                        }
                    }
                }
            }

            if let Some(classifiers) = &d.classifiers {
                let native_key = if cfg!(windows) {
                    "natives-windows"
                } else if cfg!(target_os = "macos") {
                    "natives-osx"
                } else {
                    "natives-linux"
                };
                if let Some(native) = classifiers.get(native_key) {
                    let native_jar = cache_libs.join(&native.path);
                    if let Err(e) = download(&native.url, &native_jar).await {
                        ilog!(app, log_id, "Warn downloading native {}: {}", native.path, e);
                        continue;
                    }
                    if let Err(e) = extract_natives(&native_jar, natives_dir).await {
                        ilog!(app, log_id, "Warn extracting native {}: {}", native.path, e);
                    } else {
                        ilog!(app, log_id, "Native extracted: {}", native.path);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn install_forge(
    mc_version: &str,
    instance_id: &str,
    engine: &PathBuf,
    java: &PathBuf,
    app: &AppHandle,
    log_id: &str,
) -> Result<(), String> {
    let promos_url =
        "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
    let promos: Value = reqwest::get(promos_url)
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let forge_ver = promos["promos"]
        .as_object()
        .and_then(|p| {
            p.get(&format!("{}-recommended", mc_version))
                .or_else(|| p.get(&format!("{}-latest", mc_version)))
        })
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("No Forge version found for {}", mc_version))?;
    let full_version = format!("{}-{}", mc_version, forge_ver);
    ilog!(app, log_id, "Installing Forge {}", full_version);

    let forge_work_dir = engine.join(format!("forge_installer_{}", mc_version));
    if forge_work_dir.exists() {
        let _ = fs::remove_dir_all(&forge_work_dir);
    }
    fs::create_dir_all(&forge_work_dir)
        .map_err(|e| format!("Could not create forge_installer dir: {}", e))?;

    let installer_path = forge_work_dir.join(format!("forge-{}-installer.jar", full_version));
    if !installer_path.exists() {
        let url = format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{}/forge-{}-installer.jar",
            full_version, full_version
        );
        ilog!(app, log_id, "Downloading Forge installer...");
        download(&url, &installer_path).await?;
    }

    let launcher_profiles = forge_work_dir.join("launcher_profiles.json");
    if !launcher_profiles.exists() {
        fs::write(
            &launcher_profiles,
            r#"{
            "profiles": {},
            "selectedProfile": null,
            "clientToken": "modstack",
            "authenticationDatabase": {}
        }"#,
        )
        .map_err(|e| format!("Could not create launcher_profiles.json: {}", e))?;
    }

    let java_exe = if cfg!(windows) {
        java.parent()
            .ok_or("Could not get java bin dir")?
            .join("javaw.exe")
    } else {
        java.clone()
    };
    let java_home = java_exe
        .parent()
        .and_then(|p| p.parent())
        .ok_or("Could not determine JAVA_HOME")?;

    let output = {
        let mut c = Command::new(&java_exe);
        c.current_dir(&forge_work_dir)
            .arg(format!("-Djava.home={}", java_home.display()))
            .arg("-Djava.net.preferIPv4Stack=true")
            .arg("-Djava.awt.headless=true")
            .arg("-jar")
            .arg(&installer_path)
            .arg("--installClient")
            .arg(&forge_work_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env("_JAVA_OPTIONS", "-Djava.awt.headless=true");
        #[cfg(windows)]
        c.creation_flags(0x08000000);
        c.output()
            .map_err(|e| format!("Could not run Forge installer: {}", e))?
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    ilog!(
        app,
        log_id,
        "--- Forge exit: {:?} ---",
        output.status.code()
    );
    if !output.status.success() {
        return Err(format!(
            "Forge {} failed (exit {:?})\nstdout: {}\nstderr: {}",
            full_version,
            output.status.code(),
            stdout,
            stderr
        ));
    }

    let forge_versions_src = forge_work_dir.join("versions");
    if forge_versions_src.exists() {
        copy_dir_all(&forge_versions_src, &engine.join("versions"))
            .map_err(|e| format!("Error moving Forge versions: {}", e))?;
    }

    let forge_libs_src = forge_work_dir.join("libraries");
    let engine_libs_dest = engine.join("libraries").join(instance_id).join(mc_version);

    if engine_libs_dest.exists() {
        let _ = fs::remove_dir_all(&engine_libs_dest);
    }
    fs::create_dir_all(&engine_libs_dest)
        .map_err(|e| format!("Could not create Forge libs dir: {}", e))?;

    if forge_libs_src.exists() {
        copy_dir_all(&forge_libs_src, &engine_libs_dest)
            .map_err(|e| format!("Error moving Forge libraries: {}", e))?;
    }

    let _ = fs::remove_dir_all(&forge_work_dir);
    ilog!(app, log_id, "Forge {} installed successfully", full_version);
    Ok(())
}

fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            if dst_path.exists() {
                let _ = fs::remove_file(&dst_path);
            }
            fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

async fn install_fabric(
    version: &str,
    engine_libs: &PathBuf,
    app: &AppHandle,
    log_id: &str,
) -> Result<(String, Vec<PathBuf>), String> {
    let loaders_url = format!("https://meta.fabricmc.net/v2/versions/loader/{}", version);
    ilog!(
        app,
        log_id,
        "Fetching Fabric loader versions for {}...",
        version
    );
    let loaders_text = reqwest::get(&loaders_url)
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let loaders: Value = serde_json::from_str(&loaders_text)
        .map_err(|e| format!("Error parsing Fabric loaders list: {}", e))?;
    let loader_version = loaders
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|l| l["loader"]["version"].as_str())
        .ok_or_else(|| format!("No Fabric loader version found for MC {}", version))?
        .to_string();
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
        version, loader_version
    );
    ilog!(app, log_id, "Downloading Fabric profile: {}", url);
    let profile_text = reqwest::get(&url)
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let profile: Value = serde_json::from_str(&profile_text)
        .map_err(|e| format!("Error parsing Fabric profile: {}", e))?;
    let main_class = profile["mainClass"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| {
            profile["mainClass"]["client"].as_str().map(|s| s.to_string())
        })
        .or_else(|| {
            profile["launcherMeta"]["mainClass"]["client"].as_str().map(|s| s.to_string())
        })
        .unwrap_or_else(|| "net.fabricmc.loader.impl.launch.knot.KnotClient".to_string());
    
    let empty_vec = vec![];
    let libs_json: Vec<Value> = if profile["libraries"].is_array() {
        profile["libraries"]
            .as_array()
            .unwrap_or(&empty_vec)
            .clone()
    } else {
        let common_libs = profile["launcherMeta"]["libraries"]["common"]
            .as_array()
            .unwrap_or(&empty_vec);
        let client_libs = profile["launcherMeta"]["libraries"]["client"]
            .as_array()
            .unwrap_or(&empty_vec);
        let mut all = vec![];
        all.extend_from_slice(common_libs);
        all.extend_from_slice(client_libs);
        all
    };
    ilog!(
        app,
        log_id,
        "Fabric libraries to download: {}",
        libs_json.len()
    );
    fs::create_dir_all(engine_libs)
        .map_err(|e| format!("Could not create fabric libs dir: {}", e))?;
    let client = reqwest::Client::new();
    let mut downloaded_paths: Vec<PathBuf> = vec![];
    for lib in &libs_json {
        let name = match lib["name"].as_str() {
            Some(n) => n,
            None => continue,
        };
        let parts: Vec<&str> = name.splitn(3, ':').collect();
        if parts.len() != 3 {
            continue;
        }
        let (group, artifact, ver) = (parts[0], parts[1], parts[2]);
        let group_path = group.replace('.', "/");
        let jar_name = format!("{}-{}.jar", artifact, ver);
        let maven_path = format!("{}/{}/{}/{}", group_path, artifact, ver, jar_name);
        let local_path = engine_libs.join(&maven_path);
        if local_path.exists() {
            downloaded_paths.push(local_path);
            continue;
        }
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let base_url = lib["url"].as_str().unwrap_or("https://maven.fabricmc.net/");
        let base_url = if base_url.ends_with('/') {
            base_url.to_string()
        } else {
            format!("{}/", base_url)
        };
        let dl_url = format!("{}{}", base_url, maven_path);
        match client.get(&dl_url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                Ok(bytes) => {
                    fs::write(&local_path, &bytes).ok();
                    downloaded_paths.push(local_path);
                    ilog!(app, log_id, "Fabric lib downloaded: {}", jar_name);
                }
                Err(e) => ilog!(
                    app,
                    log_id,
                    "Error reading bytes for Fabric lib {}: {}",
                    jar_name,
                    e
                ),
            },
            Ok(resp) => ilog!(
                app,
                log_id,
                "HTTP {} downloading Fabric lib: {}",
                resp.status(),
                jar_name
            ),
            Err(e) => ilog!(
                app,
                log_id,
                "Error downloading Fabric lib {}: {}",
                jar_name,
                e
            ),
        }
    }
    ilog!(
        app,
        log_id,
        "Fabric ready: {} libraries downloaded",
        downloaded_paths.len()
    );
    Ok((main_class, downloaded_paths))
}

#[command]
pub fn create_instance(
    name: String,
    id: String,
    base_path: String,
    loader: String,
    version: String,
    slug: Option<String>,
    state: State<'_, AppState>,
) -> String {
    let mut manager = state.instances.lock().unwrap();
    let instance = manager.create_instance(
        name,
        id,
        PathBuf::from(base_path),
        loader,
        version,
        slug,
        None,
        None,
        None,
    );
    instance.name
}

#[command]
pub fn list_instances(state: State<'_, AppState>) -> Vec<Instance> {
    let manager = state.instances.lock().unwrap();
    manager.instances.clone()
}

#[command]
pub fn get_instance_by_code(code: String, state: State<'_, AppState>) -> Result<Instance, String> {
    let manager = state.instances.lock().unwrap();
    manager
        .instances
        .iter()
        .find(|i| i.id == code || i.name == code)
        .cloned()
        .ok_or("Instance not found".into())
}

#[command]
pub async fn launch_instance_cmd(
    app: AppHandle,
    instance_id: String,
    username: String,
    uuid: String,
    token: String,
    ram: u64,
    width: i32,
    height: i32,
    fullscreen: bool,
    skin_data_url: Option<String>,
    arm_style: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let instance = {
        let manager = state.instances.lock().unwrap();
        for inst in &manager.instances {
            println!(
                "   id='{}' loader='{}' version='{}'",
                inst.id, inst.loader, inst.minecraft_version
            );
        }
        manager
            .instances
            .iter()
            .find(|i| i.id == instance_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Instance '{}' not found. Available: [{}]",
                    instance_id,
                    manager
                        .instances
                        .iter()
                        .map(|i| format!("'{}'", i.id))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?
    };

    let log_id = format!("{}-{}", instance.id, instance.slug.as_deref().unwrap_or(""));
    ilog!(&app, &log_id, "Looking up instance: '{}'", instance_id);

    let version = instance.minecraft_version.clone();
    let loader = instance.loader.to_lowercase();

    ilog!(
        &app,
        &log_id,
        "Launching '{}' | version={} | loader={}",
        instance.name,
        version,
        loader
    );

    let base = crate::commands::config::get_install_dir_path();
    let engine = base.join("engine_data");

    let instance_dir = instance.path.clone();
    fs::create_dir_all(&instance_dir).ok();

    let natives_dir = engine.join("natives").join(&version);
    fs::create_dir_all(&natives_dir).ok();

    let cache_dir = engine.join("cache").join(&instance_id).join(&version);
    let cache_libs = cache_dir.join("libraries");
    let cache_assets = cache_dir.join("assets");

    fs::create_dir_all(&cache_libs).ok();
    fs::create_dir_all(&cache_assets).ok();

    let version_dir = engine.join("versions").join(&version);
    let jar = version_dir.join(format!("{}.jar", version));
    let json = version_dir.join(format!("{}.json", version));

    fs::create_dir_all(&version_dir).ok();

    let parsed: VersionJson;

    if !jar.exists() || !json.exists() {
        ilog!(&app, &log_id, "Downloading Minecraft {}", version);
        app.emit("install-status", format!("Downloading Minecraft {}...", version)).ok();
        let manifest_url = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
        let manifest: Value = reqwest::get(manifest_url)
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let version_entry = manifest["versions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == version)
            .ok_or_else(|| format!("Version '{}' not found in manifest", version))?;
        let url = version_entry["url"].as_str().ok_or("Invalid URL")?;
        let version_json: String = reqwest::get(url)
            .await
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        fs::write(&json, &version_json).map_err(|e| e.to_string())?;
        parsed = serde_json::from_str(&version_json).map_err(|e| e.to_string())?;
        download(&parsed.downloads.client.url, &jar).await?;
        ilog!(&app, &log_id, "Minecraft {} downloaded", version);
        ilog!(&app, &log_id, "Downloading libraries and natives...");
        app.emit("install-status", "Downloading libraries...").ok();
        download_libraries(&parsed.libraries, &cache_libs, &natives_dir, &app, &log_id).await?;
        ilog!(&app, &log_id, "Libraries and natives ready");
        app.emit("install-done", &instance_id).ok();
    } else {
        ilog!(&app, &log_id, "Minecraft {} already downloaded", version);
        let data = fs::read_to_string(&json).map_err(|e| e.to_string())?;
        parsed = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        
        let natives_empty = fs::read_dir(&natives_dir)
            .map(|mut d| d.next().is_none())
            .unwrap_or(true);
        
        let libs_empty = fs::read_dir(&cache_libs)
            .map(|mut d| d.next().is_none())
            .unwrap_or(true);
        
        if natives_empty || libs_empty {
            ilog!(&app, &log_id, "Libraries/natives missing, re-downloading...");
            download_libraries(&parsed.libraries, &cache_libs, &natives_dir, &app, &log_id).await?;
            ilog!(&app, &log_id, "Libraries and natives ready");
            app.emit("install-done", &instance_id).ok();
        }
    }

    let required_java = parsed
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or(8);

    ilog!(&app, &log_id, "Required Java: {}", required_java);

    let runtime_base = base.join("runtime");
    let runtime_path = ensure_java(&runtime_base, required_java, &app, &log_id)
        .await
        .map_err(|e| e.to_string())?;
    let java = runtime_path
        .join("bin")
        .join(if cfg!(windows) { "javaw.exe" } else { "java" });

    ilog!(&app, &log_id, "Java path: {}", java.display());

    let engine_libs = engine.join("libraries").join(&instance_id).join(&version);
    fs::create_dir_all(&engine_libs).ok();

    if loader == "forge" {
        let forge_already_installed = fs::read_dir(engine.join("versions"))
            .map(|d| {
                d.flatten().any(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.starts_with(&format!("{}-", version)) && name.contains("forge")
                })
            })
            .unwrap_or(false);

        let engine_libs_empty = fs::read_dir(&engine_libs)
            .map(|mut d| d.next().is_none())
            .unwrap_or(true);

        if !forge_already_installed || engine_libs_empty {
            ilog!(
                &app,
                &log_id,
                "Reinstalling Forge cleanly for {}...",
                version
            );
            let _ = fs::remove_dir_all(&engine_libs);
            install_forge(&version, &instance_id, &engine, &java, &app, &log_id).await?;
        } else {
            ilog!(&app, &log_id, "Forge already installed for {}", version);
        }
    }

    let (fabric_main_class, fabric_libs) = if loader == "fabric" {
        ilog!(
            &app,
            &log_id,
            "Installing Fabric and downloading libraries..."
        );
        install_fabric(&version, &engine_libs, &app, &log_id).await?
    } else {
        (String::new(), vec![])
    };

    let indexes_dir = cache_assets.join("indexes");
    let objects_dir = cache_assets.join("objects");
    let virtual_dir = cache_assets.join("virtual").join(&parsed.asset_index.id);

    fs::create_dir_all(&indexes_dir).ok();
    fs::create_dir_all(&objects_dir).ok();
    fs::create_dir_all(&virtual_dir).ok();

    let index_path = indexes_dir.join(format!("{}.json", parsed.asset_index.id));
    let index_data = if !index_path.exists() {
        let data = reqwest::get(&parsed.asset_index.url)
            .await
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        fs::write(&index_path, &data).map_err(|e| e.to_string())?;
        data
    } else {
        fs::read_to_string(&index_path).map_err(|e| e.to_string())?
    };

    let json_assets: Value = serde_json::from_str(&index_data).map_err(|e| e.to_string())?;
    let objects = json_assets["objects"]
        .as_object()
        .ok_or("Invalid asset index")?;
    let total = objects.len();
    let count = AtomicUsize::new(0);
    let client = reqwest::Client::new();

    stream::iter(objects)
        .for_each_concurrent(256, |(_, obj)| {
            let client = client.clone();
            let objects_dir = objects_dir.clone();
            let app = app.clone();
            let count = &count;
            async move {
                let hash = obj["hash"].as_str().unwrap();
                let subdir = &hash[0..2];
                let asset_path = objects_dir.join(subdir).join(hash);
                if !asset_path.exists() {
                    let url = format!(
                        "https://resources.download.minecraft.net/{}/{}",
                        subdir, hash
                    );
                    if let Ok(resp) = client.get(&url).send().await {
                        if let Ok(bytes) = resp.bytes().await {
                            if let Some(parent) = asset_path.parent() {
                                fs::create_dir_all(parent).ok();
                            }
                            fs::write(&asset_path, &bytes).ok();
                        }
                    }
                }
                let c = count.fetch_add(1, Ordering::Relaxed) + 1;
                app.emit("asset-progress", format!("{}/{}", c, total)).ok();
            }
        })
        .await;

    ilog!(&app, &log_id, "Assets complete");
    
    let is_offline = token == "none" || token.is_empty();
    let user_type = if is_offline { "Legacy" } else { "msa" };
    let effective_token = if is_offline {
        "0".to_string()
    } else {
        token.clone()
    };
    let effective_uuid = if is_offline {
        offline_uuid(&username)
    } else {
        uuid.clone()
    };

    ilog!(
        &app,
        &log_id,
        "User type: {} | username: {} | uuid: {}",
        user_type,
        username,
        effective_uuid
    );

    let skin_agent_arg = if is_offline {
        prepare_offline_skin(
            skin_data_url.as_deref().unwrap_or(""),
            arm_style.as_deref().unwrap_or("wide"),
            &effective_uuid,
            &username,
            &engine,
            &app,
            &log_id,
        )
        .await
    } else {
        None
    };

    if skin_agent_arg.is_some() {
        ilog!(&app, &log_id, "Skin injection ready (authlib-injector)");
    }

    let mut cmd = Command::new(&java);
    cmd.current_dir(&instance_dir);
    cmd.arg(format!("-Xmx{}M", ram));
    cmd.arg("-XX:+UnlockExperimentalVMOptions");
    cmd.arg("-XX:+UseG1GC");
    cmd.arg("-Dos.name=Windows 10");
    cmd.arg("-Dos.version=10.0");
    cmd.arg("-Dorg.lwjgl.util.NoChecks=true");
    cmd.arg("-Dorg.lwjgl.system.allocator=system");
    cmd.arg(format!(
        "-Djava.library.path={}",
        natives_dir.to_string_lossy()
    ));

    if let Some(ref agent_arg) = skin_agent_arg {
        cmd.arg(agent_arg);
    }

    if loader == "forge" {
        let versions_dir = engine.join("versions");

        let forge_version_entry = fs::read_dir(&versions_dir)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .find(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with(&format!("{}-", version)) && name.contains("forge")
            })
            .ok_or_else(|| format!("Forge version directory not found for {}.", version))?;

        let forge_dir = forge_version_entry.path();
        let forge_version_name = forge_dir.file_name().unwrap().to_string_lossy().to_string();
        let forge_json_path = forge_dir.join(format!("{}.json", forge_version_name));

        let forge_json_str = fs::read_to_string(&forge_json_path)
            .map_err(|e| format!("Could not read forge JSON: {}", e))?;
        let forge_json_full: Value = serde_json::from_str(&forge_json_str)
            .map_err(|e| format!("Could not parse forge JSON: {}", e))?;
        let forge_json: ForgeVersionJson = serde_json::from_str(&forge_json_str)
            .map_err(|e| format!("Could not parse forge JSON typed: {}", e))?;

        let sep = classpath_separator();
        let engine_libs_str = engine_libs.to_string_lossy().to_string();

        ilog!(&app, &log_id, "Forge version: {}", forge_version_name);

        if let Some(libs) = forge_json_full["libraries"].as_array() {
            for lib in libs {
                if !is_value_library_allowed(lib) {
                    continue;
                }
                if let Some(path_str) = value_library_artifact_path(lib) {
                    let local = artifact_path(&engine_libs, &path_str);
                    if local.exists() {
                        continue;
                    }
                    if let Some(parent) = local.parent() {
                        fs::create_dir_all(parent).ok();
                    }
                    if let Some(url_str) = value_library_download_url(lib, &path_str) {
                        if let Err(e) = download(&url_str, &local).await {
                            ilog!(
                                &app,
                                &log_id,
                                "Warn downloading Forge lib {}: {}",
                                path_str,
                                e
                            );
                        }
                    } else {
                        ilog!(
                            &app,
                            &log_id,
                            "Warn missing Forge lib without download URL: {}",
                            path_str
                        );
                    }
                }
            }
            ilog!(&app, &log_id, "Forge libraries verified");
        }

        if let Some(args) = &forge_json.arguments {
            if let Some(jvm_args) = &args.jvm {
                for arg in jvm_args {
                    if let Some(s) = arg.as_str() {
                        let expanded = s
                            .replace("${library_directory}", &engine_libs_str)
                            .replace("${classpath_separator}", sep)
                            .replace("${version_name}", &forge_version_name);
                        cmd.arg(expanded);
                    } else if let Some(obj) = arg.as_object() {
                        if let Some(val) = obj.get("value") {
                            if let Some(s) = val.as_str() {
                                cmd.arg(
                                    s.replace("${library_directory}", &engine_libs_str)
                                        .replace("${classpath_separator}", sep)
                                        .replace("${version_name}", &forge_version_name),
                                );
                            } else if let Some(arr) = val.as_array() {
                                for v in arr {
                                    if let Some(s) = v.as_str() {
                                        cmd.arg(
                                            s.replace("${library_directory}", &engine_libs_str)
                                                .replace("${classpath_separator}", sep)
                                                .replace("${version_name}", &forge_version_name),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut forge_artifact_ids: HashSet<String> = HashSet::new();
        if let Some(libs) = forge_json_full["libraries"].as_array() {
            for lib in libs {
                if let Some(name) = lib["name"].as_str() {
                    if let Some(ga) = library_ga(name) {
                        forge_artifact_ids.insert(ga);
                    }
                }
            }
        }
        let declared_forge_artifact_ids = forge_artifact_ids.clone();

        let mut cp_parts: Vec<String> = Vec::new();
        let mut seen_cp: HashSet<String> = HashSet::new();
        push_classpath_path(&mut cp_parts, &mut seen_cp, jar.clone());

        for lib in &parsed.libraries {
            if !should_use_manifest_library(lib) {
                continue;
            }
            if let Some(name) = lib.name.as_deref() {
                if let Some(ga) = library_ga(name) {
                    if declared_forge_artifact_ids.contains(&ga) {
                        continue;
                    }
                }
            }
            if let Some(path_str) = manifest_library_artifact_path(lib) {
                push_classpath_path(
                    &mut cp_parts,
                    &mut seen_cp,
                    artifact_path(&cache_libs, &path_str),
                );
            }
        }

        if let Some(libs) = forge_json_full["libraries"].as_array() {
            for lib in libs {
                if !is_value_library_allowed(lib) {
                    continue;
                }
                if let Some(path_str) = value_library_artifact_path(lib) {
                    push_classpath_path(
                        &mut cp_parts,
                        &mut seen_cp,
                        artifact_path(&engine_libs, &path_str),
                    );
                }
            }
        }

        let classpath = cp_parts.join(sep);
        ilog!(&app, &log_id, "CLASSPATH: {}", classpath);

        let argfile_content = format!("-cp\n{}\n{}", classpath, &forge_json.main_class);
        let argfile_path = instance_dir.join(format!("forge_launch_{}.txt", instance_id));
        ilog!(&app, &log_id, "main_class: {}", &forge_json.main_class);

        fs::write(&argfile_path, &argfile_content)
            .map_err(|e| format!("Could not write argfile: {}", e))?;
        cmd.arg(format!("@{}", argfile_path.display()));

        cmd.args([
            "--gameDir",
            instance_dir.to_str().unwrap(),
            "--assetsDir",
            cache_assets.to_str().unwrap(),
            "--assetIndex",
            &parsed.asset_index.id,
            "--username",
            &username,
            "--uuid",
            &effective_uuid,
            "--accessToken",
            &effective_token,
            "--userType",
            user_type,
            "--version",
            &forge_version_name,
            "--width",
            &width.to_string(),
            "--height",
            &height.to_string(),
            "--fml.libraryDirectory",
            &engine_libs_str,
        ]);

        if let Some(args) = &forge_json.arguments {
            if let Some(game_args) = &args.game {
                for arg in game_args {
                    if let Some(s) = arg.as_str() {
                        cmd.arg(s);
                    }
                }
            }
        }
    } else if loader == "fabric" {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let mut cp_parts: Vec<String> = vec![jar.to_string_lossy().to_string()];

        if cache_libs.exists() {
            for entry in WalkDir::new(&cache_libs).into_iter().filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().map(|e| e == "jar").unwrap_or(false) {
                    let path_str = path.to_string_lossy().to_string();
                    if !cp_parts.contains(&path_str) {
                        cp_parts.push(path_str);
                    }
                }
            }
        }

        for lib_path in &fabric_libs {
            if lib_path.exists() {
                let path_str = lib_path.to_string_lossy().to_string();
                if !cp_parts.contains(&path_str) {
                    cp_parts.push(path_str);
                }
            }
        }

        if engine_libs.exists() {
            for entry in WalkDir::new(&engine_libs).into_iter().filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().map(|e| e == "jar").unwrap_or(false) {
                    let path_str = path.to_string_lossy().to_string();
                    if !cp_parts.contains(&path_str) {
                        cp_parts.push(path_str);
                    }
                }
            }
        }

        let classpath = cp_parts.join(sep);
        let main_class = if fabric_main_class.is_empty() {
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        } else {
            &fabric_main_class
        };

        ilog!(&app, &log_id, "Fabric main_class: {}", main_class);
        ilog!(&app, &log_id, "Fabric classpath entries: {}", cp_parts.len());

        cmd.args([
            "-cp",
            &classpath,
            main_class,
            "--gameDir",
            instance_dir.to_str().unwrap(),
            "--assetsDir",
            cache_assets.to_str().unwrap(),
            "--assetIndex",
            &parsed.asset_index.id,
            "--username",
            &username,
            "--uuid",
            &effective_uuid,
            "--accessToken",
            &effective_token,
            "--userType",
            user_type,
            "--version",
            &version,
            "--width",
            &width.to_string(),
            "--height",
            &height.to_string(),
        ]);
    } else {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let mut cp_parts: Vec<String> = vec![jar.to_string_lossy().to_string()];

        if cache_libs.exists() {
            for entry in WalkDir::new(&cache_libs).into_iter().filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().map(|e| e == "jar").unwrap_or(false) {
                    cp_parts.push(path.to_string_lossy().to_string());
                }
            }
        }

        let classpath = cp_parts.join(sep);

        ilog!(&app, &log_id, "Vanilla classpath entries: {}", cp_parts.len());

        cmd.args([
            "-cp",
            &classpath,
            "net.minecraft.client.main.Main",
            "--gameDir",
            instance_dir.to_str().unwrap(),
            "--assetsDir",
            cache_assets.to_str().unwrap(),
            "--assetIndex",
            &parsed.asset_index.id,
            "--username",
            &username,
            "--uuid",
            &effective_uuid,
            "--accessToken",
            &effective_token,
            "--userType",
            user_type,
            "--version",
            &version,
            "--width",
            &width.to_string(),
            "--height",
            &height.to_string(),
        ]);
    }

    if fullscreen {
        cmd.arg("--fullscreen");
    }

    ilog!(&app, &log_id, "Launching Minecraft...");

    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = cmd
        .output()
        .map_err(|e| format!("Error launching Minecraft: {}", e))?;

    ilog!(&app, &log_id, "EXIT CODE: {:?}", output.status.code());
    ilog!(
        &app,
        &log_id,
        "STDERR: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    ilog!(
        &app,
        &log_id,
        "STDOUT: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    app.emit("minecraft-closed", ()).ok();
    Ok(())
}

async fn download_authlib_injector(path: &PathBuf, app: &AppHandle, log_id: &str) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    ilog!(app, log_id, "Descargando authlib-injector...");
    let client = reqwest::Client::builder()
        .user_agent("modstack-launcher/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    let release: serde_json::Value = client
        .get("https://api.github.com/repos/yushijinhun/authlib-injector/releases/latest")
        .send()
        .await
        .map_err(|e| format!("authlib-injector info: {}", e))?
        .json()
        .await
        .map_err(|e| format!("authlib-injector parse: {}", e))?;
    let url = release["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"].as_str().map(|n| n.ends_with(".jar")).unwrap_or(false)))
        .and_then(|a| a["browser_download_url"].as_str())
        .ok_or("No se encontró URL de descarga de authlib-injector")?
        .to_string();
    let bytes = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("authlib-injector download: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("authlib-injector bytes: {}", e))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(path, &bytes).map_err(|e| format!("authlib-injector write: {}", e))?;
    ilog!(app, log_id, "authlib-injector guardado en {}", path.display());
    Ok(())
}

async fn prepare_offline_skin(
    data_url: &str,
    arm_style: &str,
    uuid: &str,
    username: &str,
    engine: &PathBuf,
    app: &AppHandle,
    log_id: &str,
) -> Option<String> {
    if data_url.is_empty() {
        return None;
    }
    let base64_part = data_url.split(',').nth(1)?;
    let skin_bytes = B64.decode(base64_part).ok()?;
    if skin_bytes.is_empty() {
        return None;
    }
    let port = crate::skin_server::start_skin_server(skin_bytes, uuid, username, arm_style)
        .await
        .map_err(|e| ilog!(app, log_id, "Skin server error: {}", e))
        .ok()?;
    let authlib_jar = engine.join("authlib-injector.jar");
    if let Err(e) = download_authlib_injector(&authlib_jar, app, log_id).await {
        ilog!(app, log_id, "authlib-injector no disponible: {}", e);
        return None;
    }
    Some(format!(
        "-javaagent:{}=http://127.0.0.1:{}",
        authlib_jar.display(),
        port
    ))
}

fn build_classpath_no_jar(lib_dirs: &[&PathBuf], exclude: &HashSet<String>) -> String {
    let mut paths: Vec<String> = vec![];
    for dir in lib_dirs {
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(dir).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().map(|e| e == "jar").unwrap_or(false) {
                let fname = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !exclude.contains(&fname) {
                    paths.push(path.to_string_lossy().to_string());
                }
            }
        }
    }
    paths.join(if cfg!(windows) { ";" } else { ":" })
}

fn build_classpath_exclude(
    lib_dirs: &[&PathBuf],
    jar: &PathBuf,
    exclude: &HashSet<String>,
) -> String {
    let mut paths = vec![jar.to_string_lossy().to_string()];
    for dir in lib_dirs {
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(dir).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().map(|e| e == "jar").unwrap_or(false) {
                let fname = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !exclude.contains(&fname) {
                    paths.push(path.to_string_lossy().to_string());
                }
            }
        }
    }
    paths.join(if cfg!(windows) { ";" } else { ":" })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalInstance {
    pub id: String,
    pub title: String,
    pub minecraft_version: String,
    pub loader: String,
    pub icon_path: Option<String>,
    pub background_path: Option<String>,
    pub created_at: i64,
}

fn instances_root(_app: &AppHandle) -> PathBuf {
    crate::commands::config::get_install_dir_path().join("instances")
}

fn instance_dir(app: &AppHandle, id: &str) -> PathBuf {
    instances_root(app).join(id)
}

fn instance_json_path(app: &AppHandle, id: &str) -> PathBuf {
    instance_dir(app, id).join("instance.json")
}

fn selected_id_path(app: &AppHandle) -> PathBuf {
    instances_root(app).join(".selected")
}

fn img_ext(src: &PathBuf) -> &'static str {
    match src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "jpg",
        _ => "png",
    }
}

fn copy_image(src: &str, dest_dir: &PathBuf, name: &str) -> Result<String, String> {
    let src_path = PathBuf::from(src);
    if !src_path.exists() {
        return Err(format!("File not found: {}", src));
    }
    let ext = img_ext(&src_path);
    let dest = dest_dir.join(format!("{}.{}", name, ext));
    fs::copy(&src_path, &dest).map_err(|e| format!("Error copying image: {}", e))?;
    Ok(dest.to_string_lossy().to_string())
}

fn init_instance_dirs(dir: &PathBuf) -> Result<(), String> {
    for sub in &["mods", "config", "saves", "resourcepacks", "shaderpacks"] {
        fs::create_dir_all(dir.join(sub)).map_err(|e| format!("Error creating {}: {}", sub, e))?;
    }
    Ok(())
}

#[command]
pub fn load_local_instances(app: AppHandle) -> Vec<LocalInstance> {
    let root = instances_root(&app);
    if !root.exists() {
        return vec![];
    }
    let mut list: Vec<LocalInstance> = fs::read_dir(&root)
        .unwrap_or_else(|_| return std::fs::read_dir(".").unwrap())
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let json = e.path().join("instance.json");
            let raw = fs::read_to_string(&json).ok()?;
            serde_json::from_str::<LocalInstance>(&raw).ok()
        })
        .collect();
    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    list
}

#[command]
pub fn add_local_instance(
    app: AppHandle,
    mut instance: LocalInstance,
    icon_src: Option<String>,
    background_src: Option<String>,
) -> Result<LocalInstance, String> {
    let dir = instance_dir(&app, &instance.id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    init_instance_dirs(&dir)?;
    if let Some(src) = icon_src.filter(|s| !s.is_empty()) {
        match copy_image(&src, &dir, "icon") {
            Ok(dest) => instance.icon_path = Some(dest),
            Err(e) => eprintln!("Warn: could not copy icon: {}", e),
        }
    }
    if let Some(src) = background_src.filter(|s| !s.is_empty()) {
        match copy_image(&src, &dir, "background") {
            Ok(dest) => instance.background_path = Some(dest),
            Err(e) => eprintln!("Warn: could not copy background: {}", e),
        }
    }
    let json = serde_json::to_string_pretty(&instance).map_err(|e| e.to_string())?;
    fs::write(instance_json_path(&app, &instance.id), json).map_err(|e| e.to_string())?;
    Ok(instance)
}

#[command]
pub fn remove_local_instance(app: AppHandle, id: String) -> Result<(), String> {
    let dir = instance_dir(&app, &id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let sel = get_selected_local_instance_id(app.clone());
    if sel.as_deref() == Some(&id) {
        set_selected_local_instance_id(app, None).ok();
    }
    Ok(())
}

#[command]
pub fn update_local_instance(
    app: AppHandle,
    id: String,
    title: Option<String>,
    minecraft_version: Option<String>,
    loader: Option<String>,
    icon_src: Option<String>,
    background_src: Option<String>,
    clear_icon: bool,
    clear_background: bool,
) -> Result<LocalInstance, String> {
    let json_path = instance_json_path(&app, &id);
    let raw = fs::read_to_string(&json_path).map_err(|_| format!("Instance '{}' not found", id))?;
    let mut inst: LocalInstance = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let dir = instance_dir(&app, &id);
    if let Some(v) = title {
        inst.title = v;
    }
    if let Some(v) = minecraft_version {
        inst.minecraft_version = v;
    }
    if let Some(v) = loader {
        inst.loader = v;
    }
    if clear_icon {
        if let Some(ref p) = inst.icon_path {
            let _ = fs::remove_file(p);
        }
        inst.icon_path = None;
    } else if let Some(src) = icon_src.filter(|s| !s.is_empty()) {
        if let Some(ref old) = inst.icon_path {
            let _ = fs::remove_file(old);
        }
        match copy_image(&src, &dir, "icon") {
            Ok(dest) => inst.icon_path = Some(dest),
            Err(e) => eprintln!("Warn: icon: {}", e),
        }
    }
    if clear_background {
        if let Some(ref p) = inst.background_path {
            let _ = fs::remove_file(p);
        }
        inst.background_path = None;
    } else if let Some(src) = background_src.filter(|s| !s.is_empty()) {
        if let Some(ref old) = inst.background_path {
            let _ = fs::remove_file(old);
        }
        match copy_image(&src, &dir, "background") {
            Ok(dest) => inst.background_path = Some(dest),
            Err(e) => eprintln!("Warn: background: {}", e),
        }
    }
    let json = serde_json::to_string_pretty(&inst).map_err(|e| e.to_string())?;
    fs::write(&json_path, json).map_err(|e| e.to_string())?;
    Ok(inst)
}

#[command]
pub fn get_selected_local_instance_id(app: AppHandle) -> Option<String> {
    let path = selected_id_path(&app);
    fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[command]
pub fn set_selected_local_instance_id(app: AppHandle, id: Option<String>) -> Result<(), String> {
    let path = selected_id_path(&app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    match id {
        Some(val) => fs::write(&path, val).map_err(|e| e.to_string()),
        None => {
            if path.exists() {
                fs::remove_file(&path).map_err(|e| e.to_string())
            } else {
                Ok(())
            }
        }
    }
}

#[command]
pub fn open_local_instance_folder(app: AppHandle, id: String) -> Result<(), String> {
    let dir = instance_dir(&app, &id);
    if !dir.exists() {
        return Err(format!("Folder not found: {}", dir.display()));
    }
    open::that(&dir).map_err(|e| e.to_string())
}

#[command]
pub fn save_local_instances(_app: AppHandle, _instances: Vec<LocalInstance>) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn export_local_instance(
    app: AppHandle,
    id: String,
    options: HashMap<String, bool>,
) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;

    let dir = instance_dir(&app, &id);
    let json_path = instance_json_path(&app, &id);
    let raw = fs::read_to_string(&json_path).map_err(|_| format!("Instance '{}' not found", id))?;
    let inst: LocalInstance = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let dest_folder = app
        .dialog()
        .file()
        .set_title("Export instance")
        .blocking_pick_folder()
        .ok_or("User cancelled export")?;

    let dest_path = PathBuf::from(dest_folder.to_string());
    let export_name = format!("{}.mrstack", slugify_export(&inst.title));
    let export_path = dest_path.join(&export_name);

    let file = fs::File::create(&export_path).map_err(|e| format!("Error creating file: {}", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let zip_options =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("instance.json", zip_options)
        .map_err(|e| e.to_string())?;
    zip.write_all(raw.as_bytes()).map_err(|e| e.to_string())?;

    let logo_candidates: &[&str] = &["icons/icon.png", "icons/32x32.png", "icons/128x128.png"];
    let search_roots: Vec<PathBuf> = {
        let mut roots = vec![];
        if let Ok(res) = app.path().resource_dir() {
            roots.push(res);
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                roots.push(parent.to_path_buf());
                roots.push(parent.join("..").join("Resources"));
                roots.push(parent.join("..").join("..").join(".."));
            }
        }
        roots
    };

    'logo: for root in &search_roots {
        for candidate in logo_candidates {
            let logo_path = root.join(candidate);
            if logo_path.exists() {
                let fname = logo_path.file_name().unwrap().to_string_lossy();
                zip.start_file(format!("assets/launcher/{}", fname), zip_options)
                    .map_err(|e| e.to_string())?;
                zip.write_all(&fs::read(&logo_path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                break 'logo;
            }
        }
    }

    let zip_subdir = |zip: &mut zip::ZipWriter<fs::File>, subdir: &str| -> Result<(), String> {
        let sub_path = dir.join(subdir);
        if !sub_path.exists() {
            return Ok(());
        }
        for entry in WalkDir::new(&sub_path).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if path.is_file() {
                let relative = path
                    .strip_prefix(&dir)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                zip.start_file(&relative, zip_options)
                    .map_err(|e| e.to_string())?;
                let bytes = fs::read(path).map_err(|e| e.to_string())?;
                zip.write_all(&bytes).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    };

    if options.get("mods").copied().unwrap_or(true) {
        zip_subdir(&mut zip, "mods")?;
    }
    if options.get("config").copied().unwrap_or(true) {
        zip_subdir(&mut zip, "config")?;
    }
    if options.get("resourcepacks").copied().unwrap_or(true) {
        zip_subdir(&mut zip, "resourcepacks")?;
    }
    if options.get("shaderpacks").copied().unwrap_or(false) {
        zip_subdir(&mut zip, "shaderpacks")?;
    }
    if options.get("datapack").copied().unwrap_or(false) {
        zip_subdir(&mut zip, "datapacks")?;
    }

    if options.get("include_images").copied().unwrap_or(true) {
        for img_path_str in [&inst.icon_path, &inst.background_path]
            .into_iter()
            .flatten()
        {
            let img_path = PathBuf::from(img_path_str);
            if img_path.exists() {
                if let Some(fname) = img_path.file_name() {
                    let zip_name = format!("assets/{}", fname.to_string_lossy());
                    zip.start_file(&zip_name, zip_options)
                        .map_err(|e| e.to_string())?;
                    zip.write_all(&fs::read(&img_path).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(export_path.to_string_lossy().to_string())
}

#[command(rename_all = "camelCase")]
pub async fn import_mrstack(app: AppHandle, mrstack_path: String) -> Result<LocalInstance, String> {
    use std::io::Read;

    let zip_bytes = fs::read(&mrstack_path).map_err(|e| format!("Could not read file: {}", e))?;
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Not a valid .mrstack: {}", e))?;

    let raw_json = {
        let mut f = archive
            .by_name("instance.json")
            .map_err(|_| "Missing instance.json in archive")?;
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| e.to_string())?;
        s
    };

    let mut inst: LocalInstance =
        serde_json::from_str(&raw_json).map_err(|e| format!("Invalid instance.json: {}", e))?;

    let existing = load_local_instances(app.clone());
    if existing.iter().any(|i| i.id == inst.id) {
        inst.id = format!("{}-{}", inst.id, chrono::Utc::now().timestamp());
    }
    inst.created_at = chrono::Utc::now().timestamp_millis();

    let dest_dir = instance_dir(&app, &inst.id);
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    init_instance_dirs(&dest_dir)?;

    inst.icon_path = None;
    inst.background_path = None;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if name == "instance.json" {
            continue;
        }
        if name.starts_with("assets/launcher/") {
            continue;
        }
        if name.ends_with('/') {
            let out_path = dest_dir.join(&name);
            fs::create_dir_all(&out_path).ok();
            continue;
        }
        if name.starts_with("assets/") {
            let fname = PathBuf::from(&name)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            if fname.is_empty() {
                continue;
            }
            let out_path = dest_dir.join(&fname);
            let mut out_file = fs::File::create(&out_path)
                .map_err(|e| format!("Error creating {}: {}", fname, e))?;
            std::io::copy(&mut entry, &mut out_file).map_err(|e| e.to_string())?;
            if fname.starts_with("icon") {
                inst.icon_path = Some(out_path.to_string_lossy().to_string());
            } else if fname.starts_with("background") {
                inst.background_path = Some(out_path.to_string_lossy().to_string());
            }
            continue;
        }
        let out_path = dest_dir.join(&name);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let mut out_file =
            fs::File::create(&out_path).map_err(|e| format!("Error creating {}: {}", name, e))?;
        std::io::copy(&mut entry, &mut out_file).map_err(|e| e.to_string())?;
    }

    let updated_json = serde_json::to_string_pretty(&inst).map_err(|e| e.to_string())?;
    fs::write(instance_json_path(&app, &inst.id), updated_json).map_err(|e| e.to_string())?;
    Ok(inst)
}

fn slugify_export(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c.to_lowercase().next().unwrap()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[command]
pub async fn install_modrinth_modpack(
    app: AppHandle,
    slug: String,
    title: String,
    icon_url: Option<String>,
) -> Result<LocalInstance, String> {
    use std::io::Read;
    use std::time::{SystemTime, UNIX_EPOCH};

    let id = format!("{}", slug);
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let dir = instance_dir(&app, &id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    init_instance_dirs(&dir)?;

    let icon_path = if let Some(url) = icon_url.filter(|s| !s.is_empty()) {
        let icon_dest = dir.join("icon.png");
        match reqwest::get(&url).await {
            Ok(resp) => match resp.bytes().await {
                Ok(bytes) => {
                    fs::write(&icon_dest, &bytes).ok();
                    Some(icon_dest.to_string_lossy().to_string())
                }
                Err(_) => None,
            },
            Err(_) => None,
        }
    } else {
        None
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap_or_default();

    let project: serde_json::Value = client
        .get(format!("https://api.modrinth.com/v2/project/{}", slug))
        .header("User-Agent", "Launcher/1.0")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let mc_version = project["game_versions"]
        .as_array()
        .and_then(|v| v.last())
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let loader = project["loaders"]
        .as_array()
        .and_then(|v| v.first())
        .and_then(|v| v.as_str())
        .unwrap_or("fabric")
        .to_string();

    let background_path = {
        let gallery = project["gallery"].as_array();
        let gallery_url = gallery
            .and_then(|g| {
                g.iter()
                    .find(|img| img["featured"].as_bool().unwrap_or(false))
                    .or_else(|| g.first())
            })
            .and_then(|img| img["raw_url"].as_str().or_else(|| img["url"].as_str()))
            .map(|s| s.to_string());
        if let Some(url) = gallery_url {
            let bg_dest = dir.join("background.png");
            match client
                .get(&url)
                .header("User-Agent", "Launcher/1.0")
                .send()
                .await
            {
                Ok(resp) => match resp.bytes().await {
                    Ok(bytes) => {
                        fs::write(&bg_dest, &bytes).ok();
                        Some(bg_dest.to_string_lossy().to_string())
                    }
                    Err(_) => None,
                },
                Err(_) => None,
            }
        } else {
            None
        }
    };

    app.emit("install-status", "Fetching modpack version...")
        .ok();

    let versions: serde_json::Value = client
        .get(format!(
            "https://api.modrinth.com/v2/project/{}/version",
            slug
        ))
        .header("User-Agent", "Launcher/1.0")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let latest = versions
        .as_array()
        .and_then(|v| v.first())
        .ok_or("No modpack versions found")?;

    let mrpack_url = latest["files"]
        .as_array()
        .and_then(|files| {
            files.iter().find(|f| {
                f["filename"]
                    .as_str()
                    .map(|n| n.ends_with(".mrpack"))
                    .unwrap_or(false)
            })
        })
        .and_then(|f| f["url"].as_str())
        .ok_or("No .mrpack file found")?
        .to_string();

    app.emit("install-status", "Downloading modpack...").ok();

    let mrpack_bytes = client
        .get(&mrpack_url)
        .header("User-Agent", "Launcher/1.0")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let cursor = std::io::Cursor::new(&mrpack_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Error opening .mrpack: {}", e))?;

    let index: serde_json::Value = {
        let mut f = archive
            .by_name("modrinth.index.json")
            .map_err(|_| "Missing modrinth.index.json in .mrpack")?;
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| e.to_string())?
    };

    let files = index["files"].as_array().cloned().unwrap_or_default();
    let total = files.len();

    app.emit("install-status", "Downloading mods...").ok();
    app.emit("install-progress", format!("0/{}", total)).ok();

    let count = std::sync::atomic::AtomicUsize::new(0);

    stream::iter(files)
        .for_each_concurrent(32, |file| {
            let client = client.clone();
            let dir = dir.clone();
            let app = app.clone();
            let count = &count;
            async move {
                let path_str = file["path"].as_str().unwrap_or("").to_string();
                if path_str.is_empty() {
                    count.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                let dest = dir.join(&path_str);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).ok();
                }
                if dest.exists() {
                    let c = count.fetch_add(1, Ordering::Relaxed) + 1;
                    app.emit("install-progress", format!("{}/{}", c, total))
                        .ok();
                    return;
                }
                let downloads = file["downloads"].as_array().cloned().unwrap_or_default();
                for dl_url in downloads {
                    if let Some(url) = dl_url.as_str() {
                        if let Ok(resp) = client.get(url).send().await {
                            if let Ok(bytes) = resp.bytes().await {
                                fs::write(&dest, &bytes).ok();
                                break;
                            }
                        }
                    }
                }
                let c = count.fetch_add(1, Ordering::Relaxed) + 1;
                app.emit("install-progress", format!("{}/{}", c, total))
                    .ok();
            }
        })
        .await;

    app.emit("install-status", "Extracting overrides...").ok();

    let cursor = std::io::Cursor::new(&mrpack_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Error reopening .mrpack: {}", e))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if !name.starts_with("overrides/") {
            continue;
        }
        let rel = name.trim_start_matches("overrides/");
        if rel.is_empty() {
            continue;
        }
        let out_path = dir.join(rel);
        if name.ends_with('/') {
            fs::create_dir_all(&out_path).ok();
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).ok();
            }
            if let Ok(mut out_file) = fs::File::create(&out_path) {
                std::io::copy(&mut entry, &mut out_file).ok();
            }
        }
    }

    app.emit("install-status", "").ok();
    app.emit("install-done", &id).ok();

    let inst = LocalInstance {
        id: id.clone(),
        title,
        minecraft_version: mc_version,
        loader,
        icon_path,
        background_path,
        created_at,
    };

    let json = serde_json::to_string_pretty(&inst).map_err(|e| e.to_string())?;
    fs::write(instance_json_path(&app, &id), json).map_err(|e| e.to_string())?;
    Ok(inst)
}
