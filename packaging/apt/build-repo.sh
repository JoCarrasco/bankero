#!/usr/bin/env bash
set -euo pipefail

# Build a minimal APT repository layout.
#
# Usage:
#   packaging/apt/build-repo.sh <repo_root> <deb_path> [codename] [component] [arch]
#
# Example:
#   packaging/apt/build-repo.sh apt-repo target/debian/bankero_0.1.0_amd64.deb stable main amd64

repo_root="${1:?repo_root required}"
deb_path="${2:?deb_path required}"
codename="${3:-stable}"
component="${4:-main}"
arch="${5:-amd64}"

pkg_name="bankero"
first_letter="${pkg_name:0:1}"

mkdir -p "${repo_root}/pool/${component}/${first_letter}/${pkg_name}"
cp -f "${deb_path}" "${repo_root}/pool/${component}/${first_letter}/${pkg_name}/"

mkdir -p "${repo_root}/dists/${codename}/${component}/binary-${arch}"

# Packages index
(
  cd "${repo_root}"
  dpkg-scanpackages --arch "${arch}" "pool" /dev/null > "dists/${codename}/${component}/binary-${arch}/Packages"
)

gzip -fk "${repo_root}/dists/${codename}/${component}/binary-${arch}/Packages"

# Release file
apt-ftparchive -c packaging/apt/apt-ftparchive.conf release "${repo_root}/dists/${codename}" > "${repo_root}/dists/${codename}/Release"
