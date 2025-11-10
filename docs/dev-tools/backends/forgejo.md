# Forgejo Backend

You may install Forgejo release assets directly using the `forgejo` backend. This backend downloads release assets from Forgejo repositories and is ideal for tools that distribute pre-built binaries through Forgejo releases.

The code for this is inside of the mise repository at [`./src/backend/forgejo.rs`](https://github.com/jdx/mise/blob/main/src/backend/forgejo.rs).

## Usage

The following installs the latest version of mergiraf from a Forgejo instance (codeberg.org)
and sets it as the active version on PATH:

```sh
$ mise use -g forgejo:codeberg.org/mergiraf/mergiraf
$ mergiraf --version
mergiraf 0.15.0
```

The version will be set in `~/.config/mise/config.toml` with the following format:

```toml
[tools]
"forgejo:codeberg.org/mergiraf/mergiraf" = "latest"
```

## Tool Options

The following [tool-options](/dev-tools/#tool-options) are available for the `forgejo` backend—these
go in `[tools]` in `mise.toml`.

### Asset Autodetection

When no `asset_pattern` is specified, mise automatically selects the best asset for your platform. The system scores assets based on:

- **OS compatibility** (linux, macos, windows)
- **Architecture compatibility** (x64, arm64, x86, arm)
- **Libc variant** (gnu or musl, for Linux)
- **Archive format preference** (tar.gz, zip, etc.)
- **Build type** (avoids debug/test builds)

For most tools, you can simply install without specifying patterns:

```sh
mise install forgejo:host/user/repo
```

::: tip
The autodetection logic is implemented in [`src/backend/asset_detector.rs`](https://github.com/jdx/mise/blob/main/src/backend/asset_detector.rs), which is shared by both the GitHub and Forgejo backends.
:::

### `asset_pattern`

Specifies the pattern to match against release asset names. This is useful when there are multiple assets for your OS/arch combination or when you need to override autodetection.

```toml
[tools]
"forgejo:codeberg.org/mergiraf/mergiraf" = { version = "latest", asset_pattern = "mergiraf_*_linux_x64.tar.gz" }
```

### `version_prefix`

Specifies a custom version prefix for release tags. By default, mise handles the common `v` prefix (e.g., `v1.0.0`), but some repositories use different prefixes like `release-`, `version-`, or no prefix at all.

When `version_prefix` is configured, mise will:

- Filter available versions with the prefix and strip it
- Add the prefix when searching for releases
- Try both prefixed and non-prefixed versions during installation

```toml
[tools]
"forgejo:host/user/repo" = { version = "latest", version_prefix = "release-" }
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
[tools."forgejo:codeberg.org/mergiraf/mergiraf"]
version = "latest"

[tools."forgejo:codeberg.org/mergiraf/mergiraf".platforms]
linux-x64 = { asset_pattern = "mergiraf_*_linux_x64.tar.gz" }
macos-arm64 = { asset_pattern = "mergiraf_*_macOS_arm64.tar.gz" }
```

### `checksum`

Verify the downloaded file with a checksum:

```toml
[tools."forgejo:host/owner/repo"]
version = "1.0.0"
asset_pattern = "tool-1.0.0-x64.tar.gz"
checksum = "sha256:a1b2c3d4e5f6789..."
```

_Instead of specifying the checksum here, you can use [mise.lock](/dev-tools/mise-lock) to manage checksums._

### Platform-specific Checksums

```toml
[tools."forgejo:codeberg.org/mergiraf/mergiraf"]
version = "latest"

[tools."forgejo:codeberg.org/mergiraf/mergiraf".platforms]
linux-x64 = { asset_pattern = "mergiraf_*_linux_x64.tar.gz", checksum = "sha256:a1b2c3d4e5f6789..." }
macos-arm64 = { asset_pattern = "mergiraf_*_macOS_arm64.tar.gz", checksum = "sha256:b2c3d4e5f6789..." }
```

### `size`

Verify the downloaded asset size:

```toml
[tools]
"forgejo:codeberg.org/mergiraf/mergiraf" = { version = "latest", size = "12345678" }
```

### `strip_components`

Number of directory components to strip when extracting archives:

```toml
[tools]
"forgejo:codeberg.org/mergiraf/mergiraf" = { version = "latest", strip_components = 1 }
```

::: info
If `strip_components` is not explicitly set, mise will automatically detect when to apply `strip_components = 1`. This happens when the extracted archive contains exactly one directory at the root level and no files. This is common with tools like ripgrep that package their binaries in a versioned directory (e.g., `ripgrep-14.1.0-x86_64-unknown-linux-musl/rg`). The auto-detection ensures the binary is placed directly in the install path where mise expects it.
:::

### `bin`

Rename the downloaded binary to a specific name. This is useful when downloading single binaries that have platform-specific names:

```toml
[tools."forgejo:host/docker/compose"]
version = "2.29.1"
bin = "docker-compose"  # Rename the downloaded binary to docker-compose
```

::: info
When downloading single binaries (not archives), mise automatically removes OS/arch suffixes from the filename. For example, `docker-compose-linux-x86_64` becomes `docker-compose` automatically. Use the `bin` option only when you need a specific custom name.
:::

### `bin_path`

Specify the directory containing binaries within the extracted archive, or where to place the downloaded file. This supports templating with `{name}`, `{version}`, `{os}`, `{arch}`, and `{ext}`:

```toml
[tools."forgejo:codeberg.org/mergiraf/mergiraf"]
version = "latest"
bin_path = "{name}-{version}/bin" # expands to mergiraf-0.15.0/bin
```

**Binary path lookup order:**

1. If `bin_path` is specified, use that directory
2. If `bin_path` is not set, look for a `bin/` directory in the install path
3. If no `bin/` directory exists, search subdirectories for `bin/` directories
4. If no `bin/` directories are found, use the root of the extracted directory

### `api_url`

For self-hosted Forgejo instances, you can specify the API URL. Note that the API path is `/api/v1`, which is different from GitHub's `/api/v3`.

```toml
[tools]
"forgejo:myforgejo.com/myorg/mytool" = { version = "latest", api_url = "https://myforgejo.com/api/v1" }
```

## Self-hosted Forgejo

If you are using a self-hosted Forgejo instance, set the `api_url` tool option.

## Supported Forgejo Syntax

- **Forgejo shorthand for latest release version:** `forgejo:host/user/repo`
- **Forgejo shorthand for specific release version:** `forgejo:host/user/repo@2.40.1`

## Settings

<script setup>
import Settings from '/components/settings.vue';
</script>
<Settings child="forgejo" :level="3" />
