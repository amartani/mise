# Forgejo Backend

You may install Forgejo release assets directly using the `forgejo` backend. This backend downloads release assets from Forgejo-based forges (such as Codeberg) and is ideal for tools that distribute pre-built binaries through Forgejo releases.

Forgejo is a self-hosted, lightweight software forge that is GitHub-compatible and commonly used by open-source projects. Popular Forgejo instances include [Codeberg](https://codeberg.org).

The code for this is inside of the mise repository at [`./src/forgejo.rs`](https://github.com/jdx/mise/blob/main/src/forgejo.rs) and [`./src/backend/github.rs`](https://github.com/jdx/mise/blob/main/src/backend/github.rs).

## Usage

The following installs the latest version of mergiraf from Codeberg (a Forgejo instance)
and sets it as the active version on PATH:

```sh
$ mise use -g forgejo:codeberg.org/mergiraf/mergiraf
$ mergiraf --version
mergiraf 0.16.1
```

The version will be set in `~/.config/mise/config.toml` with the following format:

```toml
[tools]
"forgejo:codeberg.org/mergiraf/mergiraf" = "latest"
```

## Syntax

The Forgejo backend uses a special syntax that includes the forge host:

```
forgejo:<host>/<owner>/<repo>
```

For example:
- `forgejo:codeberg.org/owner/repo` - Install from Codeberg
- `forgejo:git.example.com/owner/repo` - Install from a self-hosted Forgejo instance

## Tool Options

The following [tool-options](/dev-tools/#tool-options) are available for the `forgejo` backend—these
go in `[tools]` in `mise.toml`.

### Asset Autodetection

When no `asset_pattern` is specified, mise automatically selects the best asset for your platform. The system scores assets based on:

- **OS compatibility** (linux, macos, windows)
- **Architecture compatibility** (x64, arm64, x86, arm)
- **Libc variant** (gnu or musl for Linux, msvc for Windows)
- **Archive format preference** (tar.gz, zip, etc.)
- **Build type** (avoids debug/test builds)

For most tools, you can simply install without specifying patterns:

```sh
mise install forgejo:codeberg.org/owner/repo
```

::: tip
The autodetection logic is implemented in [`src/backend/asset_detector.rs`](https://github.com/jdx/mise/blob/main/src/backend/asset_detector.rs), which is shared across the GitHub, GitLab, and Forgejo backends.
:::

### `asset_pattern`

Specifies the pattern to match against release asset names. This is useful when there are multiple assets for your OS/arch combination or when you need to override autodetection.

```toml
[tools]
"forgejo:codeberg.org/owner/tool" = { version = "latest", asset_pattern = "tool_*_linux_x64.tar.gz" }
```

### `version_prefix`

Specifies a custom version prefix for release tags. By default, mise handles the common `v` prefix (e.g., `v1.0.0`), but some repositories use different prefixes like `release-`, `version-`, or no prefix at all.

When `version_prefix` is configured, mise will:

- Filter available versions with the prefix and strip it
- Add the prefix when searching for releases
- Try both prefixed and non-prefixed versions during installation

```toml
[tools]
"forgejo:codeberg.org/owner/repo" = { version = "latest", version_prefix = "release-" }
```

**Examples:**

- With `version_prefix = "release-"`:
  - User specifies `1.0.0` → mise searches for `release-1.0.0` tag
  - Available versions show as `1.0.0` (prefix stripped)
- With `version_prefix = ""` (empty string):
  - User specifies `1.0.0` → mise searches for `1.0.0` tag (no prefix)
  - Useful for repositories that don't use any prefix

### Platform-specific Asset Patterns

For different asset patterns per platform:

```toml
[tools."forgejo:codeberg.org/owner/tool"]
version = "latest"

[tools."forgejo:codeberg.org/owner/tool".platforms]
linux-x64 = { asset_pattern = "tool_*_linux_x64.tar.gz" }
macos-arm64 = { asset_pattern = "tool_*_macOS_arm64.tar.gz" }
```

### `checksum`

Verify the downloaded file with a checksum:

```toml
[tools."forgejo:codeberg.org/owner/repo"]
version = "1.0.0"
asset_pattern = "tool-1.0.0-x64.tar.gz"
checksum = "sha256:a1b2c3d4e5f6789..."
```

_Instead of specifying the checksum here, you can use [mise.lock](/dev-tools/mise-lock) to manage checksums._

### Platform-specific Checksums

```toml
[tools."forgejo:codeberg.org/owner/tool"]
version = "latest"

[tools."forgejo:codeberg.org/owner/tool".platforms]
linux-x64 = { asset_pattern = "tool_*_linux_x64.tar.gz", checksum = "sha256:a1b2c3d4e5f6789..." }
macos-arm64 = { asset_pattern = "tool_*_macOS_arm64.tar.gz", checksum = "sha256:b2c3d4e5f6789..." }
```

### `size`

Verify the downloaded asset size:

```toml
[tools]
"forgejo:codeberg.org/owner/tool" = { version = "latest", size = "12345678" }
```

### `strip_components`

Number of directory components to strip when extracting archives:

```toml
[tools]
"forgejo:codeberg.org/owner/tool" = { version = "latest", strip_components = 1 }
```

::: info
If `strip_components` is not explicitly set, mise will automatically detect when to apply `strip_components = 1`. This happens when the extracted archive contains exactly one directory at the root level and no files. This is common with tools like ripgrep that package their binaries in a versioned directory (e.g., `tool-1.0.0-x86_64-unknown-linux-musl/tool`). The auto-detection ensures the binary is placed directly in the install path where mise expects it.
:::

### `bin`

Rename the downloaded binary to a specific name. This is useful when downloading single binaries that have platform-specific names:

```toml
[tools."forgejo:codeberg.org/owner/tool"]
version = "2.0.0"
bin = "tool"  # Rename the downloaded binary to tool
```

::: info
When downloading single binaries (not archives), mise automatically removes OS/arch suffixes from the filename. For example, `tool-linux-x86_64` becomes `tool` automatically. Use the `bin` option only when you need a specific custom name.
:::

### `bin_path`

Specify the directory containing binaries within the extracted archive, or where to place the downloaded file. This supports templating with `{name}`, `{version}`, `{os}`, `{arch}`, and `{ext}`:

```toml
[tools."forgejo:codeberg.org/owner/tool"]
version = "latest"
bin_path = "{name}-{version}/bin" # expands to tool-1.0.0/bin
```

**Binary path lookup order:**

1. If `bin_path` is specified, use that directory
2. If `bin_path` is not set, look for a `bin/` directory in the install path
3. If no `bin/` directory exists, search subdirectories for `bin/` directories
4. If no `bin/` directories are found, use the root of the extracted directory

### `api_url`

For self-hosted Forgejo instances with custom API endpoints, specify the API URL:

```toml
[tools]
"forgejo:forge.example.com/myorg/mytool" = { version = "latest", api_url = "https://forge.example.com/api/v1" }
```

::: info
By default, mise automatically constructs the API URL from the host in the forgejo backend specification. For example, `forgejo:codeberg.org/owner/repo` automatically uses `https://codeberg.org/api/v1` as the API endpoint. You only need to specify `api_url` if your Forgejo instance uses a non-standard API path.
:::

## Self-hosted Forgejo

If you are using a self-hosted Forgejo instance, the backend will automatically use the correct API endpoint based on the host you specify. Optionally, set the `MISE_FORGEJO_TOKEN` environment variable for authentication:

```sh
export MISE_FORGEJO_TOKEN="your-token"
```

## Supported Forgejo Syntax

- **Forgejo shorthand for latest release version:** `forgejo:codeberg.org/owner/repo`
- **Forgejo shorthand for specific release version:** `forgejo:codeberg.org/owner/repo@1.0.0`

## Popular Forgejo Instances

- [Codeberg](https://codeberg.org) - A community-driven forge for open-source projects
- Many self-hosted instances - Forgejo is designed to be easy to self-host

## API Compatibility

Forgejo is GitHub-compatible and uses a similar API structure. The Forgejo backend leverages this compatibility to provide the same features available in the GitHub backend, including:

- Release asset detection
- Automatic platform-specific binary selection
- Checksum verification
- Version prefix handling

## Settings

<script setup>
import Settings from '/components/settings.vue';
</script>
<Settings child="forgejo" :level="3" />
