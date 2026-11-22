#!/usr/bin/env bash
# Publish a versioned stable release and preserve the original permanent download URL.
set -euo pipefail

: "${GH_REPO:?repository required}"
: "${BUILD_SHA:?source revision required}"
version=$(tr -d '\r\n' < midi-preview/VERSION)
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Invalid stable version: $version" >&2
  exit 1
fi
tag="acid-recall-v${version}"
asset="ACID-RECALL-Windows-x64.zip"
notes="release-notes.md"
cp midi-preview/RELEASE-NOTES.md "$notes"
printf '\nBuild source: https://github.com/%s/commit/%s\n' "$GH_REPO" "$BUILD_SHA" >> "$notes"
(cd midi-preview && zip -r "../${asset}" .)

# A numbered release is a permanent snapshot. Later main builds never replace its ZIP
# or move its source tag; increasing the app version creates the next stable release.
if gh release view "$tag" > /dev/null 2>&1; then
  stable_asset=$(gh release view "$tag" --json assets \
    --jq ".assets[] | select(.name == \"${asset}\") | .name")
  if [ -z "$stable_asset" ]; then
    pinned_sha=$(gh api "repos/${GH_REPO}/commits/${tag}" --jq .sha)
    if [ "$pinned_sha" != "$BUILD_SHA" ]; then
      echo "Incomplete stable release: rerun its original build ${pinned_sha}." >&2
      exit 1
    fi
    # Recover a failed first upload only from that exact source revision. Never clobber.
    gh release upload "$tag" "$asset"
  fi
else
  # A release may not exist yet even though someone already created its tag.
  if pinned_sha=$(gh api "repos/${GH_REPO}/commits/${tag}" --jq .sha 2>/dev/null); then
    if [ "$pinned_sha" != "$BUILD_SHA" ]; then
      echo "Stable tag ${tag} already points to a different source revision." >&2
      exit 1
    fi
  fi
  gh release create "$tag" "$asset" --target "$BUILD_SHA" \
    --title "ACID RECALL ${version}" --notes-file "$notes" --latest
fi

# Keep every previously shared DOWNLOAD link working, including the historical tag name.
if gh release view windows-preview > /dev/null 2>&1; then
  gh release upload windows-preview "$asset" --clobber
  gh api --method PATCH "repos/${GH_REPO}/git/refs/tags/windows-preview" \
    -f "sha=${BUILD_SHA}" -F force=true > /dev/null
  gh release edit windows-preview --title "ACID RECALL ${version} — Windows download" \
    --prerelease=false --latest=false --notes-file "$notes"
else
  gh release create windows-preview "$asset" --target "$BUILD_SHA" \
    --title "ACID RECALL ${version} — Windows download" --notes-file "$notes" --latest=false
fi
