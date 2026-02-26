#!/usr/bin/env bash
set -euo pipefail

# Build a minimal APT repository layout.
#
# Usage:
#   packaging/apt/build-repo.sh <repo_root> <deb_path> [codename] [component] [arch]
#
# Example:
#   packaging/apt/build-repo.sh apt-repo target/debian/bankero_0.0.3_amd64.deb stable main amd64

repo_root="${1:?repo_root required}"
deb_path="${2:?deb_path required}"
codename="${3:-stable}"
component="${4:-main}"
arch="${5:-amd64}"

pkg_name="bankero"
first_letter="${pkg_name:0:1}"

mkdir -p "${repo_root}/pool/${component}/${first_letter}/${pkg_name}"
cp -f "${deb_path}" "${repo_root}/pool/${component}/${first_letter}/${pkg_name}/"

# GitHub Pages serves static files; avoid Jekyll processing and provide a simple landing page.
touch "${repo_root}/.nojekyll"
cat > "${repo_root}/index.html" <<'HTML'
<!doctype html>
<meta charset="utf-8" />
<title>Bankero APT Repository</title>
<h1>Bankero APT Repository</h1>
<p>This is a signed APT repository. Use the install instructions in the Bankero README.</p>
<p>Public key: <a href="public.gpg">public.gpg</a></p>
HTML

mkdir -p "${repo_root}/dists/${codename}/${component}/binary-${arch}"

binary_dir="${repo_root}/dists/${codename}/${component}/binary-${arch}"
packages_path="${binary_dir}/Packages"
packages_gz_path="${packages_path}.gz"

# Packages index
(
  cd "${repo_root}"
  dpkg-scanpackages --arch "${arch}" "pool" /dev/null > "dists/${codename}/${component}/binary-${arch}/Packages"
)

gzip -fk "${packages_path}"

make_by_hash() {
  local file_path="${1:?file_path required}"
  local algo="${2:?algo required}"
  local algo_upper
  local sum

  case "${algo}" in
    sha256)
      sum=$(sha256sum "${file_path}" | awk '{print $1}')
      algo_upper="SHA256"
      ;;
    sha512)
      sum=$(sha512sum "${file_path}" | awk '{print $1}')
      algo_upper="SHA512"
      ;;
    *)
      echo "Unsupported hash algorithm: ${algo}" >&2
      exit 1
      ;;
  esac

  mkdir -p "$(dirname "${file_path}")/by-hash/${algo_upper}"
  cp -f "${file_path}" "$(dirname "${file_path}")/by-hash/${algo_upper}/${sum}"
}

# Acquire-By-Hash reduces transient index mismatches on static/CDN-backed hosting.
make_by_hash "${packages_path}" sha256
make_by_hash "${packages_path}" sha512
make_by_hash "${packages_gz_path}" sha256
make_by_hash "${packages_gz_path}" sha512

# Release file
{
  echo "Acquire-By-Hash: yes"
  apt-ftparchive -c packaging/apt/apt-ftparchive.conf release "${repo_root}/dists/${codename}"
} > "${repo_root}/dists/${codename}/Release"
