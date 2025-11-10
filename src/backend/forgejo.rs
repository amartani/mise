use crate::backend::asset_detector;
use crate::backend::backend_type::BackendType;
use crate::backend::static_helpers::lookup_platform_key;
use crate::backend::static_helpers::{
    get_filename_from_url, install_artifact, template_string, try_with_v_prefix, verify_artifact,
};
use crate::cli::args::BackendArg;
use crate::config::Config;
use crate::config::Settings;
use crate::http::HTTP;
use crate::install_context::InstallContext;
use crate::toolset::ToolVersion;
use crate::toolset::ToolVersionOptions;
use crate::backend::{forge, Backend};
use async_trait::async_trait;
use eyre::Result;
use regex::Regex;
use std::fmt::Debug;
use std::sync::Arc;
use url::Url;

#[derive(Debug)]
pub struct ForgejoBackend {
    ba: Arc<BackendArg>,
}

struct ReleaseAsset {
    name: String,
    url: String,
    url_api: String,
}

#[async_trait]
impl Backend for ForgejoBackend {
    fn get_type(&self) -> BackendType {
        BackendType::Forgejo
    }

    fn ba(&self) -> &Arc<BackendArg> {
        &self.ba
    }

    async fn _list_remote_versions(&self, _config: &Arc<Config>) -> Result<Vec<String>> {
        let repo = self.repo();
        let opts = self.ba.opts();
        let api_url = self.get_api_url(&opts)?;
        let releases = forge::list_releases_from_url(api_url.as_str(), &repo).await?;
        Ok(releases
            .into_iter()
            .filter(|r| {
                opts.get("version_prefix")
                    .is_none_or(|p| r.tag_name.starts_with(p))
            })
            .map(|r| self.strip_version_prefix(&r.tag_name))
            .rev()
            .collect())
    }

    async fn install_version_(
        &self,
        ctx: &InstallContext,
        mut tv: ToolVersion,
    ) -> Result<ToolVersion> {
        let repo = self.repo();
        let opts = tv.request.options();
        let api_url = self.get_api_url(&opts)?;

        let platform_key = self.get_platform_key();
        let asset = if let Some(existing_platform) = tv.lock_platforms.get(&platform_key) {
            debug!(
                "Using existing URL from lockfile for platform {}: {}",
                platform_key,
                existing_platform.url.clone().unwrap_or_default()
            );
            ReleaseAsset {
                name: existing_platform.name.clone().unwrap_or_else(|| {
                    get_filename_from_url(existing_platform.url.as_deref().unwrap_or(""))
                }),
                url: existing_platform.url.clone().unwrap_or_default(),
                url_api: existing_platform.url_api.clone().unwrap_or_default(),
            }
        } else {
            self.resolve_asset_url(&tv, &opts, &repo, &api_url).await?
        };

        self.download_and_install(ctx, &mut tv, &asset, &opts)
            .await?;

        Ok(tv)
    }

    async fn list_bin_paths(
        &self,
        _config: &Arc<Config>,
        tv: &ToolVersion,
    ) -> Result<Vec<std::path::PathBuf>> {
        let opts = tv.request.options();
        if let Some(bin_path_template) = opts.get("bin_path") {
            let bin_path = template_string(bin_path_template, tv);
            Ok(vec![tv.install_path().join(bin_path)])
        } else {
            self.discover_bin_paths(tv)
        }
    }
}

impl ForgejoBackend {
    pub fn from_arg(ba: BackendArg) -> Self {
        Self { ba: Arc::new(ba) }
    }

    fn repo(&self) -> String {
        let repo = self.ba.tool_name();
        let repo = repo.split('/').skip(1).collect::<Vec<_>>().join("/");
        repo
    }

    fn format_asset_list<'a, I>(assets: I) -> String
    where
        I: Iterator<Item = &'a String>,
    {
        assets.cloned().collect::<Vec<_>>().join(", ")
    }

    fn get_api_url(&self, opts: &ToolVersionOptions) -> Result<String> {
        if let Some(api_url) = opts.get("api_url") {
            return Ok(api_url.clone());
        }
        let full_repo = self.ba.tool_name();
        let host = full_repo.split('/').next().unwrap();
        let url = Url::parse(&format!("https://{host}"))?;
        Ok(format!("{}api/v1", url))
    }

    async fn download_and_install(
        &self,
        ctx: &InstallContext,
        tv: &mut ToolVersion,
        asset: &ReleaseAsset,
        opts: &ToolVersionOptions,
    ) -> Result<()> {
        let filename = asset.name.clone();
        let file_path = tv.download_path().join(&filename);

        let mut op_count = 1;

        let has_checksum = lookup_platform_key(opts, "checksum")
            .or_else(|| opts.get("checksum").cloned())
            .is_some();
        if has_checksum {
            op_count += 1;
        }

        let needs_extraction = filename.ends_with(".tar.gz")
            || filename.ends_with(".tar.xz")
            || filename.ends_with(".tar.bz2")
            || filename.ends_with(".tar.zst")
            || filename.ends_with(".tgz")
            || filename.ends_with(".txz")
            || filename.ends_with(".tbz2")
            || filename.ends_with(".zip");
        if needs_extraction {
            op_count += 1;
        }

        ctx.pr.start_operations(op_count);

        let platform_key = self.get_platform_key();
        let platform_info = tv.lock_platforms.entry(platform_key).or_default();
        platform_info.name = Some(asset.name.clone());
        platform_info.url = Some(asset.url.clone());
        platform_info.url_api = Some(asset.url_api.clone());

        let url = match HTTP.head(asset.url.clone()).await {
            Ok(_) => asset.url.clone(),
            Err(_) => asset.url_api.clone(),
        };

        let headers = forge::get_headers(&url);

        ctx.pr.set_message(format!("download {filename}"));
        HTTP.download_file_with_headers(url, &file_path, &headers, Some(ctx.pr.as_ref()))
            .await?;

        verify_artifact(tv, &file_path, opts, Some(ctx.pr.as_ref()))?;
        install_artifact(tv, &file_path, opts, Some(ctx.pr.as_ref()))?;
        self.verify_checksum(ctx, tv, &file_path)?;

        Ok(())
    }

    fn discover_bin_paths(&self, tv: &ToolVersion) -> Result<Vec<std::path::PathBuf>> {
        let bin_path = tv.install_path().join("bin");
        if bin_path.exists() {
            return Ok(vec![bin_path]);
        }

        let mut paths = Vec::new();
        if let Ok(entries) = std::fs::read_dir(tv.install_path()) {
            for entry in entries.flatten() {
                let sub_bin_path = entry.path().join("bin");
                if sub_bin_path.exists() {
                    paths.push(sub_bin_path);
                }
            }
        }

        if paths.is_empty() {
            Ok(vec![tv.install_path()])
        } else {
            Ok(paths)
        }
    }

    async fn resolve_asset_url(
        &self,
        tv: &ToolVersion,
        opts: &ToolVersionOptions,
        repo: &str,
        api_url: &str,
    ) -> Result<ReleaseAsset> {
        if let Some(direct_url) = lookup_platform_key(opts, "url") {
            return Ok(ReleaseAsset {
                name: get_filename_from_url(&direct_url),
                url: direct_url.clone(),
                url_api: direct_url.clone(),
            });
        }

        let version = &tv.version;
        let version_prefix = opts.get("version_prefix").map(|s| s.as_str());
        try_with_v_prefix(version, version_prefix, |candidate| async move {
            self.resolve_forge_asset_url(tv, opts, repo, api_url, &candidate)
                .await
        })
        .await
    }

    async fn resolve_forge_asset_url(
        &self,
        tv: &ToolVersion,
        opts: &ToolVersionOptions,
        repo: &str,
        api_url: &str,
        version: &str,
    ) -> Result<ReleaseAsset> {
        let release = forge::get_release_for_url(api_url, repo, version).await?;

        let available_assets: Vec<String> = release.assets.iter().map(|a| a.name.clone()).collect();

        if let Some(pattern) = lookup_platform_key(opts, "asset_pattern")
            .or_else(|| opts.get("asset_pattern").cloned())
        {
            let templated_pattern = template_string(&pattern, tv);

            let asset = release
                .assets
                .into_iter()
                .find(|a| self.matches_pattern(&a.name, &templated_pattern))
                .ok_or_else(|| {
                    eyre::eyre!(
                        "No matching asset found for pattern: {}\nAvailable assets: {}",
                        templated_pattern,
                        Self::format_asset_list(available_assets.iter())
                    )
                })?;

            return Ok(ReleaseAsset {
                name: asset.name,
                url: asset.browser_download_url,
                url_api: asset.url.unwrap_or_default(),
            });
        }

        let asset_name = self.auto_detect_asset(&available_assets)?;
        let asset = self
            .find_asset_case_insensitive(&release.assets, &asset_name, |a| &a.name)
            .ok_or_else(|| {
                eyre::eyre!(
                    "Auto-detected asset not found: {}\nAvailable assets: {}",
                    asset_name,
                    Self::format_asset_list(available_assets.iter())
                )
            })?;

        Ok(ReleaseAsset {
            name: asset.name.clone(),
            url: asset.browser_download_url.clone(),
            url_api: asset.url.clone().unwrap_or_default(),
        })
    }


    fn auto_detect_asset(&self, available_assets: &[String]) -> Result<String> {
        let settings = Settings::get();
        let picker = asset_detector::AssetPicker::new(
            settings.os().to_string(),
            settings.arch().to_string(),
        );

        picker.pick_best_asset(available_assets).ok_or_else(|| {
            eyre::eyre!(
                "No suitable asset found for current platform ({}-{})\nAvailable assets: {}",
                settings.os(),
                settings.arch(),
                available_assets.join(", ")
            )
        })
    }

    fn find_asset_case_insensitive<'a, T>(
        &self,
        assets: &'a [T],
        target_name: &str,
        get_name: impl Fn(&T) -> &str,
    ) -> Option<&'a T> {
        assets
            .iter()
            .find(|a| get_name(a) == target_name)
            .or_else(|| {
                let target_lower = target_name.to_lowercase();
                assets
                    .iter()
                    .find(|a| get_name(a).to_lowercase() == target_lower)
            })
    }

    fn matches_pattern(&self, asset_name: &str, pattern: &str) -> bool {
        let regex_pattern = pattern
            .replace('.', "\\.")
            .replace('*', ".*")
            .replace('?', ".");

        if let Ok(re) = Regex::new(&format!("^{regex_pattern}$")) {
            re.is_match(asset_name)
        } else {
            asset_name.contains(pattern)
        }
    }

    fn strip_version_prefix(&self, tag_name: &str) -> String {
        let opts = self.ba.opts();

        if let Some(prefix) = opts.get("version_prefix") {
            if let Some(stripped) = tag_name.strip_prefix(prefix) {
                return stripped.to_string();
            }
        }

        if tag_name.starts_with('v') {
            tag_name.trim_start_matches('v').to_string()
        } else {
            tag_name.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::BackendArg;

    fn create_test_backend() -> ForgejoBackend {
        ForgejoBackend::from_arg(BackendArg::new(
            "forgejo".to_string(),
            Some("forgejo:codeberg.org/mergiraf/mergiraf".to_string()),
        ))
    }

    #[test]
    fn test_pattern_matching() {
        let backend = create_test_backend();
        assert!(backend.matches_pattern("test-v1.0.0.zip", "test-*"));
        assert!(!backend.matches_pattern("other-v1.0.0.zip", "test-*"));
    }

    #[test]
    fn test_version_prefix_functionality() {
        let mut backend = create_test_backend();

        assert_eq!(backend.strip_version_prefix("v1.0.0"), "1.0.0");
        assert_eq!(backend.strip_version_prefix("1.0.0"), "1.0.0");

        let mut opts = ToolVersionOptions::default();
        opts.opts
            .insert("version_prefix".to_string(), "release-".to_string());
        backend.ba = Arc::new(BackendArg::new_raw(
            "test".to_string(),
            Some("forgejo:codeberg.org/mergiraf/mergiraf".to_string()),
            "test".to_string(),
            Some(opts),
        ));

        assert_eq!(backend.strip_version_prefix("release-1.0.0"), "1.0.0");
        assert_eq!(backend.strip_version_prefix("1.0.0"), "1.0.0");
    }

    #[test]
    fn test_find_asset_case_insensitive() {
        let backend = create_test_backend();

        struct TestAsset {
            name: String,
        }

        let assets = vec![
            TestAsset {
                name: "tool-1.0.0-linux-x86_64.tar.gz".to_string(),
            },
            TestAsset {
                name: "tool-1.0.0-Darwin-x86_64.tar.gz".to_string(),
            },
            TestAsset {
                name: "tool-1.0.0-Windows-x86_64.zip".to_string(),
            },
        ];

        let result =
            backend.find_asset_case_insensitive(&assets, "tool-1.0.0-linux-x86_64.tar.gz", |a| {
                &a.name
            });
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "tool-1.0.0-linux-x86_64.tar.gz");

        let result = backend.find_asset_case_insensitive(
            &assets,
            "tool-1.0.0-darwin-x86_64.tar.gz", // lowercase 'd'
            |a| &a.name,
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "tool-1.0.0-Darwin-x86_64.tar.gz");

        let result = backend.find_asset_case_insensitive(
            &assets,
            "tool-1.0.0-windows-x86_64.zip", // lowercase 'w'
            |a| &a.name,
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "tool-1.0.0-Windows-x86_64.zip");

        let result =
            backend.find_asset_case_insensitive(&assets, "nonexistent-asset.tar.gz", |a| &a.name);
        assert!(result.is_none());
    }
}
