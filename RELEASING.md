# Releasing

Releases are driven by `charts/katago-server/Chart.yaml`. When its `version`
changes on `main`, the release workflow builds and pushes every image variant,
publishes the Helm chart, creates the tag `vX.Y.Z` and a GitHub release.
Cargo.toml `version`, Chart.yaml `version` and Chart.yaml `appVersion` must be
identical; CI's "Version consistency" job enforces it.

## Steps

1. Move the entries under `## [Unreleased]` in `CHANGELOG.md` into a new
   `## [X.Y.Z] - YYYY-MM-DD` section and add its link reference at the bottom.
2. Set `version = "X.Y.Z"` in `Cargo.toml` and refresh the lockfile:

   ```bash
   cargo update -p katago-server
   ```

3. Set `version: X.Y.Z` and `appVersion: "X.Y.Z"` in `charts/katago-server/Chart.yaml`.
4. Open a pull request (`chore: release X.Y.Z`). CI must be green, including the
   version consistency check.
5. Merge to `main`. The release workflow runs automatically.

## Verify

```bash
gh run list --workflow release.yml --limit 1
docker pull ghcr.io/goban-app/katago-server:X.Y.Z
docker pull ghcr.io/goban-app/katago-server:X.Y.Z-gpu
docker run --rm ghcr.io/goban-app/katago-server:X.Y.Z --version
helm repo update && helm search repo katago-server --versions | head
gh release view vX.Y.Z
```

## Versioning

Semantic versioning. Breaking API changes bump the major version; new fields,
endpoints or images bump the minor; fixes bump the patch. Adding optional
response fields is not breaking.

## If it fails

- Check the Actions tab for the failing job. Docker jobs most often fail on
  upstream downloads (KataGo source, networks); re-run the job.
- The workflow only tags after all images and the chart are published, so a
  failed run leaves no half release. Fix forward with a new patch version.
