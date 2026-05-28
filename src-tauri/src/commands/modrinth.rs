use tauri::{command, AppHandle};
use std::{fs, path::PathBuf};

fn instances_root() -> PathBuf {
    crate::commands::config::get_install_dir_path().join("instances")
}

fn content_dir(instance_id: &str, project_type: &str) -> PathBuf {
    let subdir = match project_type {
        "resourcepack" => "resourcepacks",
        "shader"       => "shaderpacks",
        "datapack"     => "datapacks",
        _              => "mods",
    };
    instances_root().join(instance_id).join(subdir)
}

#[command]
pub async fn download_mod(url: String, path: String) -> Result<String, String> {
    let bytes = reqwest::get(&url).await.map_err(|e| e.to_string())?
        .bytes().await.map_err(|e| e.to_string())?;
    fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok("Downloaded".into())
}

#[command]
pub async fn modrinth_install(
    _app: AppHandle,
    instance_id: String,
    slug: String,
    project_type: String,
    game_version: String,
    loader: Option<String>,
    version_id: Option<String>,
) -> Result<InstalledMod, String> {
    let client = reqwest::Client::new();

    install_modrinth_mod(
        &client,
        &instance_id,
        &slug,
        &project_type,
        &game_version,
        &loader,
        version_id.as_deref(),
        &mut std::collections::HashSet::new(),
    ).await
}

async fn install_modrinth_mod(
    client: &reqwest::Client,
    instance_id: &str,
    slug: &str,
    project_type: &str,
    game_version: &str,
    loader: &Option<String>,
    version_id: Option<&str>,
    visited: &mut std::collections::HashSet<String>,
) -> Result<InstalledMod, String> {
    if visited.contains(slug) {
        return Err(format!("Circular dependency detected for '{}'", slug));
    }
    visited.insert(slug.to_string());

    let url = if let Some(vid) = version_id {
        format!("https://api.modrinth.com/v2/version/{}", vid)
    } else {
        let mut u = format!(
            "https://api.modrinth.com/v2/project/{}/version?game_versions=[\"{}\"]",
            slug, game_version
        );
        if project_type == "mod" {
            if let Some(ref l) = loader {
                if !l.is_empty() { u.push_str(&format!("&loaders=[\"{}\"]", l)); }
            }
        }
        u
    };

    #[derive(serde::Deserialize)]
    struct MrDep {
        project_id: Option<String>,
        dependency_type: String,
    }
    #[derive(serde::Deserialize)]
    struct MrFile { url: String, filename: String, primary: bool }
    #[derive(serde::Deserialize)]
    struct MrVersion {
        files: Vec<MrFile>,
        dependencies: Option<Vec<MrDep>>,
        version_number: String,
        #[allow(dead_code)]
        id: String,
    }

    let mut versions: Vec<MrVersion> = if version_id.is_some() {
        let single: MrVersion = client.get(&url)
            .header("User-Agent", "ModstackApp/1.0").send().await.map_err(|e| e.to_string())?
            .json().await.map_err(|e| e.to_string())?;
        vec![single]
    } else {
        client.get(&url)
            .header("User-Agent", "ModstackApp/1.0").send().await.map_err(|e| e.to_string())?
            .json().await.map_err(|e| e.to_string())?
    };

    if versions.is_empty() && project_type == "mod" {
        let fallback = format!(
            "https://api.modrinth.com/v2/project/{}/version?game_versions=[\"{}\"]",
            slug, game_version
        );
        versions = client.get(&fallback)
            .header("User-Agent", "ModstackApp/1.0").send().await.map_err(|e| e.to_string())?
            .json().await.map_err(|e| e.to_string())?;
    }

    let version = versions.into_iter().next()
        .ok_or_else(|| format!("No version of '{}' for MC {}", slug, game_version))?;

    let file = version.files.iter().find(|f| f.primary).or_else(|| version.files.first())
        .ok_or("No files in this version")?;

    let dest_dir = content_dir(instance_id, project_type);
    fs::create_dir_all(&dest_dir).map_err(|e| format!("Error creating directory: {}", e))?;

    let dest_path = dest_dir.join(&file.filename);
    if !dest_path.exists() {
        let bytes = client.get(&file.url)
            .header("User-Agent", "ModstackApp/1.0").send().await.map_err(|e| e.to_string())?
            .bytes().await.map_err(|e| e.to_string())?;
        fs::write(&dest_path, &bytes).map_err(|e| format!("Error saving file: {}", e))?;
    }

    #[derive(serde::Deserialize)]
    struct MrProject {
        title: String,
        icon_url: Option<String>,
    }
    let project: MrProject = client
        .get(format!("https://api.modrinth.com/v2/project/{}", slug))
        .header("User-Agent", "ModstackApp/1.0")
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let installed = InstalledMod {
        id:           slug.to_string(),
        name:         project.title,
        author:       String::new(),
        version:      version.version_number,
        filename:     file.filename.clone(),
        icon_url:     project.icon_url,
        enabled:      true,
        has_update:   false,
        has_download: true,
    };

    if let Some(deps) = version.dependencies {
        for dep in deps {
            if dep.dependency_type == "required" {
                if let Some(dep_project_id) = dep.project_id {
                    Box::pin(install_modrinth_mod(
                        client,
                        instance_id,
                        &dep_project_id,
                        project_type,
                        game_version,
                        loader,
                        None,
                        visited,
                    )).await?;
                }
            }
        }
    }

    Ok(installed)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstalledMod {
    pub id:           String,
    pub name:         String,
    pub author:       String,
    pub version:      String,
    pub filename:     String,
    pub icon_url:     Option<String>,
    pub enabled:      bool,
    pub has_update:   bool,
    pub has_download: bool,
}

#[command]
pub fn get_installed_mods(
    instance_id:  String,
    project_type: String,
) -> Result<Vec<InstalledMod>, String> {
    let dir = content_dir(&instance_id, &project_type);
    if !dir.exists() { return Ok(vec![]); }

    let mut mods: Vec<InstalledMod> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".jar") || name.ends_with(".jar.disabled")
            || name.ends_with(".zip") || name.ends_with(".zip.disabled")
        })
        .map(|e| {
            let filename = e.file_name().to_string_lossy().to_string();
            let enabled  = !filename.ends_with(".disabled");

            let clean = filename
                .trim_end_matches(".disabled")
                .trim_end_matches(".jar")
                .trim_end_matches(".zip")
                .to_string();

            let meta = if filename.replace(".disabled","").ends_with(".jar") {
                read_jar_meta(&e.path())
            } else { None };

            let name = meta.as_ref()
                .and_then(|m| m.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .unwrap_or_else(|| clean.split('-').next().unwrap_or(&clean).to_string());

            let version = meta.as_ref()
                .and_then(|m| m.get("version").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .unwrap_or_default();

            let author = meta.as_ref()
                .and_then(|m| m.get("authors").and_then(|a| {
                    a.as_array()?.first().and_then(|v|
                        v.as_str().map(|s| s.to_string())
                        .or_else(|| v.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    )
                }))
                .unwrap_or_default();

            InstalledMod {
                id: clean.clone(), name, author, version,
                filename, icon_url: None,
                enabled, has_update: false, has_download: false,
            }
        })
        .collect();

    mods.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(mods)
}

fn read_jar_meta(jar_path: &std::path::Path) -> Option<serde_json::Value> {
    use std::io::Read;
    let bytes = fs::read(jar_path).ok()?;
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    for candidate in &["fabric.mod.json", "quilt.mod.json", "mcmod.info"] {
        if let Ok(mut file) = archive.by_name(candidate) {
            let mut buf = String::new();
            if file.read_to_string(&mut buf).is_ok() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&buf) {
                    if let Some(arr) = v.as_array() { return arr.first().cloned(); }
                    return Some(v);
                }
            }
        }
    }
    None
}

#[command]
pub fn toggle_mod(instance_id: String, filename: String, enabled: bool) -> Result<(), String> {
    for subdir in &["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        let base = instances_root().join(&instance_id).join(subdir);
        if !base.exists() { continue; }

        let clean_filename = filename
            .trim_end_matches(".disabled")
            .to_string();

        let on  = base.join(&clean_filename);
        let off = base.join(format!("{}.disabled", clean_filename));

        if enabled && off.exists() {
            fs::rename(&off, &on).map_err(|e| e.to_string())?;
            return Ok(());
        }
        if enabled && on.exists() {
            return Ok(());
        }
        if !enabled && on.exists() {
            fs::rename(&on, &off).map_err(|e| e.to_string())?;
            return Ok(());
        }
        if !enabled && off.exists() {
            return Ok(());
        }
    }
    Err(format!("File '{}' not found", filename))
}

#[command]
pub fn delete_mod(instance_id: String, filename: String) -> Result<(), String> {
    for subdir in &["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        let base = instances_root().join(&instance_id).join(subdir);
        let clean_filename = filename
            .trim_end_matches(".disabled")
            .to_string();
        let on  = base.join(&clean_filename);
        let off = base.join(format!("{}.disabled", clean_filename));
        if on.exists()  { fs::remove_file(&on).map_err(|e| e.to_string())?;  return Ok(()); }
        if off.exists() { fs::remove_file(&off).map_err(|e| e.to_string())?; return Ok(()); }
    }
    Err(format!("File '{}' not found", filename))
}

#[command]
pub async fn curseforge_install(
    instance_id:  String,
    mod_id:       String,
    project_type: String,
    game_version: String,
) -> Result<(), String> {
    const CF_API_KEY: &str = "$2a$10$piVONlDwyu/KXz.jZDFQ/eEdKEBmLYfEDK7vlLixtgevppSHQm06C";

    let client = reqwest::Client::new();

    let files_url = format!(
        "https://api.curseforge.com/v1/mods/{}/files?gameVersion={}&pageSize=10",
        mod_id, game_version
    );

    let resp: serde_json::Value = client.get(&files_url)
        .header("x-api-key", CF_API_KEY)
        .header("Accept", "application/json")
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| format!("Error parsing files response: {}", e))?;

    let file = resp["data"].as_array()
        .and_then(|arr| arr.first())
        .ok_or("No files found for this mod and game version")?;

    let file_id  = file["id"].as_u64().ok_or("Invalid file ID")?;
    let filename = file["fileName"].as_str().ok_or("Invalid filename")?.to_string();

    let dl_url = if let Some(url) = file["downloadUrl"].as_str() {
        url.to_string()
    } else {
        let id_str = file_id.to_string();
        let (part1, part2) = id_str.split_at(id_str.len().saturating_sub(3));
        format!(
            "https://edge.forgecdn.net/files/{}/{}/{}",
            part1, part2.parse::<u32>().unwrap_or(0), filename
        )
    };

    let dest_dir: PathBuf = content_dir(&instance_id, &project_type);
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

    let dest_path = dest_dir.join(&filename);
    if dest_path.exists() { return Ok(()); }

    let bytes = client.get(&dl_url)
        .header("x-api-key", CF_API_KEY)
        .send().await.map_err(|e| e.to_string())?
        .bytes().await.map_err(|e| e.to_string())?;

    fs::write(&dest_path, &bytes).map_err(|e| e.to_string())?;
    Ok(())
}